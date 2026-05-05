//! Server startup: CLI, disk check, EC pool init, HTTP serve

pub mod cmd;
pub mod disk;
pub mod banner;
pub mod run;

pub use run::ServerConfig;
