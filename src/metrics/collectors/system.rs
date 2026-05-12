//! System-level metrics: drive, memory, CPU, process.
//!
//! Drive metrics are populated once from `DiskInfo` at startup.
//! Memory and process uptime are refreshed via direct OS calls every
//! 30 s.  CPU is a placeholder until periodic polling is implemented.
//!
//! # Safety
//!
//! Memory collection on macOS/FreeBSD requires `libc::sysctl` and
//! `libc::host_statistics64` which are marked `unsafe`.  The calls
//! are confined to this module and all invariants are documented.

#![allow(unsafe_code)]

use std::sync::Arc;
use std::time::Instant;

use prometheus::{Gauge, GaugeVec, Opts};

use crate::metrics::registry::MetricsGroup;
use crate::metrics::types::{MetricInfo, MetricType};
use crate::storage::DiskInfo;

// ── Drive (static snapshot) ──────────────────────────────────────────────

pub fn drive_group(disk_infos: &[DiskInfo]) -> MetricsGroup {
    let infos = vec![
        MetricInfo {
            name: "minio_system_drive_total_bytes".into(),
            help: "Total bytes on the drive".into(),
            metric_type: MetricType::Gauge,
            labels: vec!["drive".into()],
        },
        MetricInfo {
            name: "minio_system_drive_free_bytes".into(),
            help: "Free bytes on the drive".into(),
            metric_type: MetricType::Gauge,
            labels: vec!["drive".into()],
        },
    ];

    let group = MetricsGroup::new("/system/drive", infos);

    let total = GaugeVec::new(
        Opts::new("minio_system_drive_total_bytes", "Total bytes on the drive"),
        &["drive"],
    )
    .unwrap();
    let free = GaugeVec::new(
        Opts::new("minio_system_drive_free_bytes", "Free bytes on the drive"),
        &["drive"],
    )
    .unwrap();

    for d in disk_infos {
        total.with_label_values(&[&d.mount_path]).set(d.total as f64);
        free.with_label_values(&[&d.mount_path]).set(d.free as f64);
    }

    group.register(Box::new(total));
    group.register(Box::new(free));
    group
}

// ── Memory / process (direct OS calls) ────────────────────────────────────

/// Collection of dynamic gauges refreshed directly from the OS.
pub struct SystemCollector {
    memory_total: Gauge,
    memory_free: Gauge,
    start_time: Gauge,
    uptime: Gauge,
    server_start: Instant,
}

impl SystemCollector {
    pub fn new() -> (MetricsGroup, MetricsGroup, Self) {
        // memory
        let mem_total = Gauge::with_opts(Opts::new(
            "minio_system_memory_total_bytes",
            "Total system memory in bytes",
        ))
        .unwrap();
        let mem_free = Gauge::with_opts(Opts::new(
            "minio_system_memory_free_bytes",
            "Free system memory in bytes",
        ))
        .unwrap();

        let mem_group = MetricsGroup::new(
            "/system/memory",
            vec![
                MetricInfo {
                    name: "minio_system_memory_total_bytes".into(),
                    help: "Total system memory in bytes".into(),
                    metric_type: MetricType::Gauge,
                    labels: vec![],
                },
                MetricInfo {
                    name: "minio_system_memory_free_bytes".into(),
                    help: "Free system memory in bytes".into(),
                    metric_type: MetricType::Gauge,
                    labels: vec![],
                },
            ],
        );
        mem_group.register(Box::new(mem_total.clone()));
        mem_group.register(Box::new(mem_free.clone()));

        // process
        let start = Gauge::with_opts(Opts::new(
            "minio_system_process_starttime_seconds",
            "Process start time as Unix timestamp",
        ))
        .unwrap();
        let up = Gauge::with_opts(Opts::new(
            "minio_system_process_uptime_seconds",
            "Process uptime in seconds",
        ))
        .unwrap();

        let proc_group = MetricsGroup::new(
            "/system/process",
            vec![
                MetricInfo {
                    name: "minio_system_process_starttime_seconds".into(),
                    help: "Process start time as Unix timestamp".into(),
                    metric_type: MetricType::Gauge,
                    labels: vec![],
                },
                MetricInfo {
                    name: "minio_system_process_uptime_seconds".into(),
                    help: "Process uptime in seconds".into(),
                    metric_type: MetricType::Gauge,
                    labels: vec![],
                },
            ],
        );
        proc_group.register(Box::new(start.clone()));
        proc_group.register(Box::new(up.clone()));

        let server_start = Instant::now();
        let collector = Self {
            memory_total: mem_total,
            memory_free: mem_free,
            start_time: start,
            uptime: up,
            server_start,
        };
        collector.do_refresh();

        (mem_group, proc_group, collector)
    }

    /// Refresh memory and uptime gauges. Call periodically (every 30 s).
    pub fn refresh(&self) {
        self.do_refresh();
    }

    fn do_refresh(&self) {
        self.refresh_memory();
        self.refresh_uptime();
    }

    fn refresh_memory(&self) {
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
        {
            // Total physical memory via sysctl
            let total = get_total_memory();
            self.memory_total.set(total as f64);

            // Free/available memory
            let free = get_free_memory();
            self.memory_free.set(free as f64);
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "freebsd")))]
        {
            // Other platforms: best-effort, keep at zero
        }
    }

    fn refresh_uptime(&self) {
        let uptime_secs = self.server_start.elapsed().as_secs_f64();
        self.uptime.set(uptime_secs);
        // Approximate start time (reverse from uptime; accurate enough for metrics)
        #[allow(clippy::cast_possible_truncation)]
        {
            self.start_time
                .set(std::time::UNIX_EPOCH.elapsed().unwrap_or_default().as_secs_f64()
                    - uptime_secs);
        }
    }
}

// ── OS memory helpers ────────────────────────────────────────────────────

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn get_total_memory() -> u64 {
    let mut mib = [libc::CTL_HW as i32, libc::HW_MEMSIZE as i32];
    let mut val: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: mib, val, and len are correctly sized for the sysctl call.
    unsafe {
        if libc::sysctl(mib.as_mut_ptr(), 2, &mut val as *mut _ as _, &mut len, std::ptr::null_mut(), 0)
            == 0
        {
            val
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
fn get_total_memory() -> u64 {
    // Read /proc/meminfo for MemTotal (in kB)
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    0
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
#[allow(deprecated)]
fn get_free_memory() -> u64 {
    // Use vm_statistics64 to get free + inactive pages (available memory)
    let port = unsafe { libc::mach_host_self() };
    let mut count: u32 = libc::HOST_VM_INFO64_COUNT as _;
    let mut stat: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let page_size = {
        let mut val: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let mut mib = [libc::CTL_HW as i32, libc::HW_PAGESIZE as i32];
        // SAFETY: correctly sized sysctl call
        unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut val as *mut _ as _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        val
    };

    // SAFETY: host_statistics64 with correct parameters
    let ret = unsafe {
        libc::host_statistics64(
            port,
            libc::HOST_VM_INFO64,
            &mut stat as *mut _ as _,
            &mut count,
        )
    };
    if ret == libc::KERN_SUCCESS {
        let free_pages =
            u64::from(stat.free_count) + u64::from(stat.inactive_count);
        free_pages * page_size
    } else {
        0
    }
}

#[cfg(target_os = "linux")]
fn get_free_memory() -> u64 {
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemAvailable:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    0
}

// ── Convenience ──────────────────────────────────────────────────────────

/// Build all system groups and the dynamic `SystemCollector`.
pub fn build_system_groups(
    disk_infos: &[DiskInfo],
) -> (Vec<Arc<MetricsGroup>>, Arc<SystemCollector>) {
    let (mem_group, proc_group, collector) = SystemCollector::new();

    // CPU is a static placeholder until per-core polling is implemented.
    let cpu_gauge = Gauge::with_opts(Opts::new(
        "minio_system_cpu_usage_percent",
        "CPU usage percentage (0-100)",
    ))
    .unwrap();
    cpu_gauge.set(0.0);
    let cpu_group = MetricsGroup::new(
        "/system/cpu",
        vec![MetricInfo {
            name: "minio_system_cpu_usage_percent".into(),
            help: "CPU usage percentage (0–100)".into(),
            metric_type: MetricType::Gauge,
            labels: vec![],
        }],
    );
    cpu_group.register(Box::new(cpu_gauge));

    let groups: Vec<Arc<MetricsGroup>> = vec![
        Arc::new(drive_group(disk_infos)),
        Arc::new(mem_group),
        Arc::new(cpu_group),
        Arc::new(proc_group),
    ];
    (groups, Arc::new(collector))
}
