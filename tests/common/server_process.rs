use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Manages a minio server subprocess for integration testing.
///
/// On creation it starts the server with the given number of disk directories
/// inside a temporary directory.  The server is automatically killed and the
/// temp directory cleaned up when `TestServer` is dropped.
pub struct TestServer {
    child: Option<Child>,
    pub addr: String,
    _data_dir: TempDir,
}

impl TestServer {
    /// Start a minio server subprocess with `disk_count` disk directories.
    ///
    /// A free TCP port is acquired via the probe-bind-drop technique, ensuring
    /// tests can run in parallel without port conflicts.  The function blocks
    /// (synchronous poll with 200 ms interval, 30 s timeout) until the server
    /// is accepting connections.
    pub async fn start(disk_count: usize) -> Self {
        // 1. Create temporary data directory
        let data_dir = TempDir::new().expect("create temp data dir");

        // 2. Create disk sub-directories
        let disk_paths: Vec<String> = (0..disk_count)
            .map(|i| {
                let p = data_dir.path().join(format!("disk{}", i));
                std::fs::create_dir_all(&p)
                    .expect("create disk sub-directory");
                p.to_string_lossy().to_string()
            })
            .collect();

        // 3. Probe a free port (probe-bind-drop — safe on macOS/Linux)
        let probe = TcpListener::bind("127.0.0.1:0")
            .expect("failed to probe free port");
        let port = probe.local_addr().expect("get probed port").port();
        drop(probe);
        // Brief yield so the OS fully releases the port
        std::thread::sleep(Duration::from_millis(100));

        let addr = format!("127.0.0.1:{}", port);

        // 4. Build the command
        let mut cmd = Command::new("./target/debug/minio");
        cmd.arg("server");
        for d in &disk_paths {
            cmd.arg(d);
        }
        cmd.arg("--address").arg(&addr);
        cmd.env("MINIO_ROOT_USER", "minioadmin");
        cmd.env("MINIO_ROOT_PASSWORD", "minioadmin");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().expect("start minio server subprocess");

        // 5. Wait for the server to start listening
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match TcpStream::connect(&addr) {
                Ok(_) => break,
                Err(_) => {
                    if Instant::now() > deadline {
                        panic!("timed out waiting for minio server on {}", addr);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }

        Self {
            child: Some(child),
            addr,
            _data_dir: data_dir,
        }
    }

    /// Base HTTP URL (e.g. `http://127.0.0.1:9000`)
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
