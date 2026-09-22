//! Collect only explicit local selections; no remote source filesystem access.
use std::sync::Arc;
use aiks_core::{providers::SessionProvider,service::SnapshotReceipt};
use super::{ClientError,ClientResult,CollectorOutbox,ServiceClient};

pub struct CollectionPolicy {
    pub source_key:String,
    pub include_ids:Option<Vec<String>>,
    pub exclude_ids:Vec<String>,
    pub redact_secrets:bool,
    pub max_sessions:usize,
}
impl Default for CollectionPolicy {
    fn default()->Self {Self{source_key:"default".into(),include_ids:None,exclude_ids:Vec::new(),redact_secrets:true,max_sessions:100}}
}
#[derive(Default)]
pub struct CollectionReport {
    pub discovered:usize,pub queued:usize,pub unchanged:usize,pub deferred:usize,pub excluded:usize,pub failed:usize,pub complete:bool,
}
pub async fn collect_provider(_provider:&dyn SessionProvider,_client:&ServiceClient,_outbox:Arc<CollectorOutbox>,_policy:&CollectionPolicy)->ClientResult<CollectionReport>{Err(ClientError::Retryable)}
pub async fn deliver_one(_client:&ServiceClient,_outbox:Arc<CollectorOutbox>,_now:u64)->ClientResult<Option<SnapshotReceipt>>{Err(ClientError::Retryable)}
