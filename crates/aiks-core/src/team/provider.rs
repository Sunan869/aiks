//! Trusted identity-provider boundary. These inputs are not authenticated wire DTOs.
use std::{future::Future, pin::Pin};
use super::{DirectorySnapshot, TeamError};

#[derive(Clone)]
pub struct ExternalLogin {
    pub corp_id: String,
    pub union_id: String,
    pub external_user_id: String,
    pub display_name: String,
}

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, TeamError>> + Send + 'a>>;

pub trait IdentityProvider: Send + Sync {
    fn exchange_code<'a>(&'a self, code: &'a str) -> ProviderFuture<'a, ExternalLogin>;
    fn directory<'a>(&'a self, scope: &'a [String]) -> ProviderFuture<'a, DirectorySnapshot>;
}
