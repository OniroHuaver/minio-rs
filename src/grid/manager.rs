use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::grid::AuthValidateFn;
use crate::grid::connection::{Connection, auth_validate_token};
use crate::grid::handler::{HandlerRegistry, SingleHandler};

/// Configuration for Manager initialization.
#[derive(Clone)]
pub struct ManagerOptions {
    /// Local host:port.
    pub local_host: String,
    /// Auth token shared across connections.
    pub auth_token: String,
    /// Known peer addresses for full-mesh discovery (optional).
    pub hosts: Vec<String>,
    /// Custom auth validator. If set, overrides token-based auth.
    pub auth_fn: Option<AuthValidateFn>,
}

/// Manages connections to all remote peers and handler registration.
///
/// Thread-safe, cheap to clone (wraps `Arc<ManagerInner>`).
#[derive(Clone)]
pub struct Manager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    /// Local node identifier.
    local_id: [u8; 16],
    /// Local host:port.
    local_host: String,
    /// Auth token shared across all connections.
    auth_token: String,
    /// Known peer addresses.
    hosts: Vec<String>,
    /// Custom auth validator.
    auth_fn: Option<AuthValidateFn>,
    /// All registered handlers.
    registry: Arc<HandlerRegistry>,
    /// Active connections: remote → Connection.
    connections: RwLock<HashMap<String, Connection>>,
}

impl Manager {
    /// Create a new Manager with simple token auth.
    pub fn new(local_host: String, auth_token: String) -> Self {
        Self::with_options(ManagerOptions {
            local_host,
            auth_token,
            hosts: Vec::new(),
            auth_fn: None,
        })
    }

    /// Create a new Manager with full options.
    pub fn with_options(opts: ManagerOptions) -> Self {
        let local_uuid = Uuid::new_v4();
        let inner = ManagerInner {
            local_id: local_uuid.into_bytes(),
            local_host: opts.local_host,
            auth_token: opts.auth_token,
            hosts: opts.hosts,
            auth_fn: opts.auth_fn,
            registry: Arc::new(HandlerRegistry::new()),
            connections: RwLock::new(HashMap::new()),
        };
        Manager {
            inner: Arc::new(inner),
        }
    }

    /// Register a single-request handler.
    pub async fn register_single_handler(&self, handler: SingleHandler) {
        let mut singles = self.inner.registry.singles.write().await;
        singles.insert(handler.id, handler.handler);
    }

    /// Get or create a connection to the given remote.
    pub async fn connection(&self, remote: &str) -> Connection {
        let conns = self.inner.connections.read().await;
        if let Some(conn) = conns.get(remote) {
            return conn.clone();
        }
        drop(conns); // Release read lock before taking write lock.

        let mut conns = self.inner.connections.write().await;
        let local_host = &self.inner.local_host;
        let auth_token = &self.inner.auth_token;
        let registry = &self.inner.registry;
        let local_id = self.inner.local_id;
        let auth_validate: AuthValidateFn = match self.inner.auth_fn.clone() {
            Some(f) => f,
            None => auth_validate_token(auth_token.clone()),
        };
        conns
            .entry(remote.to_string())
            .or_insert_with(|| {
                let is_client = Connection::should_connect(local_host, remote);
                Connection::new(
                    local_id,
                    local_host.clone(),
                    remote.to_string(),
                    is_client,
                    auth_token.clone(),
                    auth_validate.clone(),
                    registry.clone(),
                )
            })
            .clone()
    }

    /// Validate an auth token against the stored token or custom auth_fn.
    pub fn validate_auth(&self, token: &str) -> bool {
        if let Some(ref auth_fn) = self.inner.auth_fn {
            return auth_fn(token);
        }
        token == self.inner.auth_token
    }

    /// Known peer hosts.
    pub fn hosts(&self) -> &[String] {
        &self.inner.hosts
    }

    /// Returns a reference to the handler registry (for spawned connections).
    #[allow(dead_code)]
    pub(crate) fn registry(&self) -> Arc<HandlerRegistry> {
        self.inner.registry.clone()
    }

    /// Local node ID.
    pub fn local_id(&self) -> [u8; 16] {
        self.inner.local_id
    }

    /// Local host.
    pub fn local_host(&self) -> &str {
        &self.inner.local_host
    }

    /// Number of active connections.
    pub async fn connection_count(&self) -> usize {
        self.inner.connections.read().await.len()
    }

    /// Remove a pooled connection to `remote` (e.g. after a permanent failure).
    pub async fn remove_connection(&self, remote: &str) -> Option<Connection> {
        self.inner.connections.write().await.remove(remote)
    }

    /// Drop any cached [`Connection`] to `remote` and return a fresh one (re-dial / new handshake).
    pub async fn replace_connection(&self, remote: &str) -> Connection {
        self.remove_connection(remote).await;
        self.connection(remote).await
    }
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager")
            .field("local_id", &self.inner.local_id)
            .field("local_host", &self.inner.local_host)
            .field("hosts", &self.inner.hosts)
            .field("has_custom_auth", &self.inner.auth_fn.is_some())
            .field("auth_token_len", &self.inner.auth_token.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_auth_token_match() {
        let mgr = Manager::new("node1:9000".into(), "secret".into());
        assert!(mgr.validate_auth("secret"));
        assert!(!mgr.validate_auth("wrong"));
    }

    #[test]
    fn test_validate_auth_custom_fn() {
        let mgr = Manager::with_options(ManagerOptions {
            local_host: "node1:9000".into(),
            auth_token: "unused".into(),
            hosts: vec![],
            auth_fn: Some(Arc::new(|token: &str| token == "debug-token")),
        });
        assert!(mgr.validate_auth("debug-token"));
        assert!(!mgr.validate_auth("secret"));
    }

    #[tokio::test]
    async fn test_remove_connection() {
        let mgr = Manager::new("n1:1".into(), "t".into());
        let _c = mgr.connection("peer:1").await;
        assert_eq!(mgr.connection_count().await, 1);
        assert!(mgr.remove_connection("peer:1").await.is_some());
        assert_eq!(mgr.connection_count().await, 0);
    }
}
