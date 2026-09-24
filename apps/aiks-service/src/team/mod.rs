//! Team configuration and transport adapters. No team listener is enabled here.
mod check;
pub mod config;
pub mod dingtalk;
pub mod directory_worker;
pub mod secrets;
pub mod server;

mod auth_http;
pub mod auth_routes;
pub mod business_routes;
mod content_routes;
mod import_routes;
mod middleware;
mod share_routes;
