use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::grid::error::RemoteErr;
use crate::grid::message::HandlerId;

/// A single-request handler closure.
///
/// Takes raw payload bytes, returns response bytes or a remote error.
pub type SingleHandlerFn = Arc<
    dyn Fn(
            Vec<u8>,
        )
            -> Pin<Box<dyn Future<Output = Result<Vec<u8>, RemoteErr>> + Send>>
        + Send
        + Sync,
>;

/// Wraps a closure into a `SingleHandlerFn`.
pub fn single_handler_fn<F, Fut>(f: F) -> SingleHandlerFn
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<u8>, RemoteErr>> + Send + 'static,
{
    Arc::new(move |payload| Box::pin(f(payload)))
}

/// Handler registry, shared between Manager and Connection.
pub(crate) struct HandlerRegistry {
    pub singles: tokio::sync::RwLock<HashMap<HandlerId, SingleHandlerFn>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            singles: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

/// A registered single-request handler.
pub struct SingleHandler {
    pub id: HandlerId,
    pub handler: SingleHandlerFn,
}

impl SingleHandler {
    pub fn new(id: HandlerId, handler: SingleHandlerFn) -> Self {
        Self { id, handler }
    }
}
