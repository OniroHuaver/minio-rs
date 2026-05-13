//! Server startup: CLI, disk check, EC pool init, HTTP serve

pub mod banner;
pub mod cmd;
pub mod disk;
pub mod lock;
pub mod run;
pub mod signal;

pub use run::ServerConfig;
