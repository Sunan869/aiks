use crate::service_client::{
    valid_id, ClientError, ClientResult, ServiceClient, ServiceConnection, TargetIdentity,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct TeamEndpoint {
    origin: Url,
}
impl TeamEndpoint {
    pub fn parse(value: &str) -> ClientResult<Self> {
        let origin = Url::parse(value).map_err(|_| ClientError::InvalidInput)?;
        if origin.scheme() != "https"
            || origin.port_or_known_default().is_none_or(|v| v == 0)
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
            || origin.host_str().is_none()
        {
            return Err(ClientError::InvalidInput);
        }
        Ok(Self { origin })
    }
    pub fn origin(&self) -> String {
        self.origin.origin().ascii_serialization()
    }
    pub fn url(&self, path: &str) -> ClientResult<Url> {
        if !path.starts_with('/') || path.contains("..") || path.contains('?') || path.contains('#')
        {
            return Err(ClientError::InvalidInput);
        }
        self.origin
            .join(path)
            .map_err(|_| ClientError::InvalidInput)
    }
    pub fn validate_browser_handoff(&self, value: &str) -> ClientResult<Url> {
        let url = Url::parse(value).map_err(|_| ClientError::InvalidResponse)?;
        if url.scheme() != "https"
            || url.origin() != self.origin.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path() != "/api/v1/auth/dingtalk/browser"
        {
            return Err(ClientError::InvalidResponse);
        }
        let pairs = url.query_pairs().collect::<Vec<_>>();
        if pairs.len() != 2
            || !pairs.iter().any(|(k, v)| k == "attempt_id" && valid_id(v))
            || !pairs.iter().any(|(k, v)| k == "launch" && valid_token(v))
            || pairs.iter().any(|(k, _)| {
                matches!(
                    k.to_ascii_lowercase().as_str(),
                    "access_token" | "refresh_token" | "authorization" | "token"
                )
            })
        {
            return Err(ClientError::InvalidResponse);
        }
        Ok(url)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamIdentity {
    pub instance_id: String,
    pub company_id: String,
    pub user_id: String,
    pub space_id: String,
    pub display_name: String,
}
impl TeamIdentity {
    pub fn validate(&self) -> ClientResult<()> {
        if [
            self.instance_id.as_str(),
            self.company_id.as_str(),
            self.user_id.as_str(),
            self.space_id.as_str(),
        ]
        .iter()
        .any(|value| !valid_id(value))
            || self.display_name.trim().is_empty()
            || self.display_name.chars().count() > 512
            || self.display_name.chars().any(char::is_control)
        {
            return Err(ClientError::InvalidResponse);
        }
        Ok(())
    }
    pub fn target(&self) -> ClientResult<TargetIdentity> {
        TargetIdentity::team(
            &self.instance_id,
            &self.company_id,
            &self.user_id,
            &self.space_id,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub connection_id: String,
    pub origin: String,
    pub active_identity: Option<TeamIdentity>,
}
impl ConnectionRecord {
    pub fn new(origin: &str) -> ClientResult<Self> {
        let endpoint = TeamEndpoint::parse(origin)?;
        Ok(Self {
            connection_id: uuid::Uuid::new_v4().to_string(),
            origin: endpoint.origin(),
            active_identity: None,
        })
    }
    pub fn validate(&self) -> ClientResult<()> {
        if !valid_id(&self.connection_id) {
            return Err(ClientError::InvalidInput);
        }
        TeamEndpoint::parse(&self.origin)?;
        if let Some(identity) = &self.active_identity {
            identity.validate()?;
        }
        Ok(())
    }
    pub fn endpoint(&self) -> ClientResult<TeamEndpoint> {
        TeamEndpoint::parse(&self.origin)
    }
}

pub fn service_client(
    record: &ConnectionRecord,
    identity: &TeamIdentity,
    access_token: &str,
) -> ClientResult<ServiceClient> {
    identity.validate()?;
    let connection = ServiceConnection::team(
        &record.origin,
        &identity.instance_id,
        &identity.company_id,
        &identity.user_id,
        &identity.space_id,
        access_token,
    )?;
    ServiceClient::new(connection)
}

pub(crate) fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
