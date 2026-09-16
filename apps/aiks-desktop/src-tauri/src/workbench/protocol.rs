use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

pub const BRIDGE_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceMode {
    Knowledge,
    Session,
}

impl WorkspaceMode {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "knowledge" => Ok(Self::Knowledge),
            "session" => Ok(Self::Session),
            other => bail!("unsupported workspace mode: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum WorkbenchAction {
    ShowKnowledgeRoot,
    ShowSessionRoot,
    OpenDocument { doc_id: String },
    OpenBlock { doc_id: String, block_id: String },
    SetWorkspaceMode { mode: WorkspaceMode },
    ShowBacklinks { block_id: String },
    ShowOutline,
    ShowDatabase,
    ShowGraph,
    ShowSearch,
    RefreshDocument { doc_id: String },
}

pub fn validate_protocol_version(version: u16) -> anyhow::Result<()> {
    if version != BRIDGE_PROTOCOL_VERSION {
        bail!("unsupported bridge protocol version: {version}; expected {BRIDGE_PROTOCOL_VERSION}");
    }
    Ok(())
}

pub fn validate_identifier(field: &str, value: &str) -> anyhow::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} is required");
    }
    Ok(value.to_string())
}

pub fn validate_loopback_origin(origin: &str) -> anyhow::Result<Url> {
    let url = Url::parse(origin).with_context(|| format!("invalid workbench origin: {origin}"))?;
    if url.scheme() != "http" {
        bail!("workbench origin must use http");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("workbench origin must not contain credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("workbench origin must not contain query or fragment");
    }
    if !matches!(url.path(), "" | "/") {
        bail!("workbench origin must not contain a path");
    }

    let is_loopback = match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if !is_loopback {
        bail!("workbench origin must resolve to loopback");
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_bridge_protocol_v1() {
        assert!(validate_protocol_version(1).is_ok());
        assert!(validate_protocol_version(2).is_err());
        assert!(validate_protocol_version(0).is_err());
    }

    #[test]
    fn accepts_only_loopback_http_origins() {
        assert!(validate_loopback_origin("http://127.0.0.1:6806").is_ok());
        assert!(validate_loopback_origin("http://localhost:6806").is_ok());
        assert!(validate_loopback_origin("http://[::1]:6806").is_ok());
        assert!(validate_loopback_origin("https://example.com").is_err());
        assert!(validate_loopback_origin("http://192.0.2.1:6806").is_err());
    }

    #[test]
    fn rejects_missing_document_and_block_ids() {
        assert!(validate_identifier("doc_id", "20260916000100-abcdefg").is_ok());
        assert!(validate_identifier("doc_id", "").is_err());
        assert!(validate_identifier("block_id", "   ").is_err());
    }

    #[test]
    fn workspace_mode_parser_rejects_unknown_modes() {
        assert_eq!(
            WorkspaceMode::parse("knowledge").unwrap(),
            WorkspaceMode::Knowledge
        );
        assert_eq!(
            WorkspaceMode::parse("session").unwrap(),
            WorkspaceMode::Session
        );
        assert!(WorkspaceMode::parse("admin").is_err());
        assert!(WorkspaceMode::parse("").is_err());
    }
}
