//! Authenticated personal loopback transport; Core owns all business behavior.
mod auth;
mod config;
mod error;
mod routes;
pub mod bootstrap;
pub use auth::LocalAuth;
pub use config::ServiceConfig;
pub use routes::build_router;
pub use aiks_core::service::ServiceRuntime;
