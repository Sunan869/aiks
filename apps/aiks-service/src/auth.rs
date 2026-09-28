use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use subtle::ConstantTimeEq;

/// No Debug/Serialize: the authentication hash is not a client capability.
pub struct LocalAuth {
    digest: [u8; 32],
    instance_id: String,
    authority: Option<String>,
    boot_nonce: String,
}
impl LocalAuth {
    pub fn new(token: &str, instance_id: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
            "Authentication requires 32 random bytes encoded as hex"
        );
        anyhow::ensure!(
            !instance_id.is_empty() && instance_id.len() <= 128,
            "Invalid instance identity"
        );
        Ok(Self {
            digest: Sha256::digest(token.as_bytes()).into(),
            instance_id: instance_id.into(),
            authority: None,
            boot_nonce: uuid::Uuid::new_v4().to_string(),
        })
    }
    pub fn with_authority(mut self, address: SocketAddr) -> anyhow::Result<Self> {
        anyhow::ensure!(
            crate::config::allowed_ip(address.ip()) && address.port() != 0,
            "Invalid bound loopback authority"
        );
        self.authority = Some(address.to_string());
        Ok(self)
    }
    pub fn boot_nonce(&self) -> &str {
        &self.boot_nonce
    }
    pub(crate) fn check(&self, headers: &HeaderMap, public: bool) -> bool {
        // Browser-originated requests are never accepted by this bearer-token API.
        // Personal mode additionally pins Host to the owned loopback listener.
        // Collector mode intentionally leaves authority unset so a reverse proxy
        // may forward any configured public Host while the bearer + instance id
        // remain mandatory for every non-public request.
        if headers.contains_key("origin") {
            return false;
        }
        match self.authority.as_deref() {
            Some(authority) if one(headers, "host") != Some(authority) => return false,
            None if one(headers, "host").is_none() => return false,
            _ => {}
        }
        if public {
            return true;
        }
        if one(headers, "x-aiks-instance-id") != Some(self.instance_id.as_str()) {
            return false;
        }
        let Some(token) = one(headers, "authorization").and_then(|v| v.strip_prefix("Bearer "))
        else {
            return false;
        };
        if token.len() != 64 {
            return false;
        }
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        bool::from(self.digest.ct_eq(&digest))
    }
}
fn one<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.to_str().ok()
}
