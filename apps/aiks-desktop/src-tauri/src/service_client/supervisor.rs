use super::{ClientError, ClientResult};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadyHandshake {
    pub api_version: u32,
    pub instance_id: String,
    pub space_id: String,
    pub boot_nonce: String,
    pub address: String,
}
impl ReadyHandshake {
    pub fn parse(_frame: &str, _expected: Option<&str>) -> ClientResult<Self> {
        Err(ClientError::InvalidResponse)
    }
}
