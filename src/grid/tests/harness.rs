//! TestGrid — multi-node WebSocket sandbox (reference implementation).
//!
//! Starts real HTTP/WS servers and establishes WebSocket connections between nodes.
//! Currently a reference skeleton; full wiring requires resolving Connection identity
//! on incoming WebSocket upgrades.
//!
//! Usage (when fully implemented):
//! ```rust,ignore
//! let tg = TestGrid::new(3).await;
//! tg.connect_all().await;
//! ```

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::grid::handler::HandlerRegistry;
use crate::grid::manager::{Manager, ManagerOptions};

struct Node {
    manager: Manager,
    _host: String,
    #[allow(dead_code)]
    addr: SocketAddr,
    registry: Arc<HandlerRegistry>,
}

/// Multi-node grid sandbox.
pub struct TestGrid {
    nodes: Vec<Node>,
}

impl TestGrid {
    /// Create N nodes with dynamic ports and running HTTP servers.
    #[allow(dead_code)]
    pub async fn new(n: usize) -> Self {
        assert!(n >= 2, "TestGrid requires at least 2 nodes");

        let mut addrs = Vec::with_capacity(n);
        for _ in 0..n {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            addrs.push(listener.local_addr().unwrap());
        }
        let hosts: Vec<String> = addrs.iter().map(|a| a.to_string()).collect();

        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            let registry = Arc::new(HandlerRegistry::new());
            let host = hosts[i].clone();
            let (ready_tx, mut ready_rx) = oneshot::channel::<()>();

            let mgr = Manager::with_options(ManagerOptions {
                local_host: host.clone(),
                auth_token: "testgrid".into(),
                hosts: hosts.clone(),
                auth_fn: None,
            });

            let app = Router::new().route(
                "/grid",
                get(|_: WebSocketUpgrade| async {
                    // Upgrade handler placeholder.
                }),
            );

            let listener = TcpListener::bind(addrs[i]).await.unwrap();
            tokio::spawn(async move {
                let _ = ready_tx.send(());
                axum::serve(listener, app).await.unwrap();
            });

            let _ = ready_rx.try_recv();
            tokio::task::yield_now().await;

            nodes.push(Node {
                manager: mgr,
                _host: host,
                addr: addrs[i],
                registry,
            });
        }

        TestGrid { nodes }
    }

    #[allow(dead_code)]
    pub fn manager(&self, idx: usize) -> Manager {
        self.nodes[idx].manager.clone()
    }

    #[allow(dead_code)]
    pub fn registry(&self, idx: usize) -> Arc<HandlerRegistry> {
        self.nodes[idx].registry.clone()
    }

    #[allow(dead_code)]
    pub async fn wait_all_connected(&self, _timeout: Duration) -> bool {
        // In full implementation: poll all connection states.
        false
    }
}
