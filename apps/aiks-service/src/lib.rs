//! Authenticated personal loopback transport; Core owns all business behavior.
mod auth;
pub mod bootstrap;
mod config;
mod error;
pub mod model_credentials;
mod routes;
pub mod team;
pub use aiks_core::service::ServiceRuntime;
pub use auth::LocalAuth;
pub use config::ServiceConfig;
pub use routes::build_router;
