//! Server startup banner — formatted box with version, endpoint, disk list, and EC config

use crate::storage::DiskInfo;

/// Print a formatted startup banner via `tracing::info!`.
pub fn print_banner(address: &str, console_address: Option<&str>, disk_infos: &[DiskInfo]) {
    let disk_count = disk_infos.len();
    let (data, parity, wq, rq) = compute_ec(disk_count);

    let version = crate::VERSION;
    let mut lines = Vec::new();

    // Top border
    lines.push(format!("┌{}┐", "─".repeat(56)));

    // Title
    lines.push(format!("│{:^56}│", format!("minio-rs  {version}")));

    // Blank separator
    lines.push(format!("│{:^56}│", ""));

    // Endpoint
    lines.push(format!("│  Endpoint:  http://{:<40}│", address));

    // Console
    match console_address {
        Some(addr) => {
            lines.push(format!("│  Console:   http://{:<40}│", addr));
        }
        None => {
            lines.push(format!("│  Console:   (disabled in Phase 1){:<24}│", ""));
        }
    }

    // Blank separator
    lines.push(format!("│{:^56}│", ""));

    // Disk list header
    lines.push(format!("│  Disks:{:>50}│", ""));

    // Each disk entry
    for info in disk_infos {
        let status = if info.online { "up" } else { "down" };
        let formatted = if info.formatted { "yes" } else { "no" };
        lines.push(format!(
            "│    {:<20}  {:<4}  {:>8}  formatted: {:<10}│",
            info.mount_path,
            status,
            format_size(info.total),
            formatted,
        ));
    }

    // Blank separator
    lines.push(format!("│{:^56}│", ""));

    // EC configuration
    lines.push(format!("│  Erasure Configuration:{:>34}│", ""));
    lines.push(format!("│    Drives:  {:<46}│", disk_count));
    lines.push(format!("│    Data:    {:<46}│", data));
    lines.push(format!("│    Parity:  {:<46}│", parity));
    lines.push(format!(
        "│    Write Quorum:  {}/{}  {:<34}│",
        wq, disk_count, ""
    ));
    lines.push(format!(
        "│    Read Quorum:   {}/{}  {:<34}│",
        rq, disk_count, ""
    ));

    let efficiency = if disk_count > 0 {
        data as f64 * 100.0 / disk_count as f64
    } else {
        0.0
    };
    lines.push(format!(
        "│    Storage Usage:  {:.1}%  (usable / total){:<23}│",
        efficiency, "",
    ));

    // Blank separator
    lines.push(format!("│{:^56}│", ""));

    // Health endpoint
    lines.push(format!(
        "│  Status:  http://{:<43}│",
        format!("{address}/minio/health/live"),
    ));

    // Bottom border
    lines.push(format!("└{}┘", "─".repeat(56)));

    tracing::info!("\n{}", lines.join("\n"));
}

/// EC parameter calculation matching `Erasure::with_default_parity`.
fn compute_ec(disk_count: usize) -> (usize, usize, usize, usize) {
    if disk_count < 3 {
        return (disk_count, 0, disk_count, disk_count); // standalone: no EC
    }
    let parity = match disk_count {
        0..=5 => 2,
        6..=7 => 3,
        _ => 4,
    };
    let data = disk_count - parity;
    let write_quorum = if data == parity { data } else { data + 1 };
    let read_quorum = disk_count - parity;
    (data, parity, write_quorum, read_quorum)
}

/// Human-readable size formatting.
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut i = 0;
    while size >= 1024.0 && i < UNITS.len() - 1 {
        size /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", size, UNITS[i])
}
