//! Server startup: CLI, disk check, EC pool init, HTTP serve

pub mod cmd;
pub mod disk;
pub mod banner;
pub mod run;
pub mod signal;
pub mod lock;

pub use run::ServerConfig;
