//! Client-only transport and upload state; never opens the business StateDb.
use std::{fmt, path::Path};
use aiks_core::{model::SourceKind, service::{SnapshotReceipt, SnapshotSubmission}};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError { InvalidInput, WrongInstance, Unauthorized, NotFound, Conflict, Retryable, Busy, TooLarge, Storage, InvalidResponse }
impl fmt::Display for ClientError { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { write!(f,"{self:?}") } }
impl std::error::Error for ClientError {}
pub type ClientResult<T> = Result<T,ClientError>;

#[derive(Clone)]
pub struct PendingSubmission { submission:SnapshotSubmission }
impl PendingSubmission {
    pub fn new(submission:SnapshotSubmission)->ClientResult<Self> { Ok(Self{submission}) }
    pub fn submission(&self)->&SnapshotSubmission { &self.submission }
    pub fn payload_hash(&self)->&str { "pending-implementation" }
}
pub struct ServiceConnection;
impl ServiceConnection {
    pub fn local(_url:&str,_instance:&str,_space:&str,_token:&str)->ClientResult<Self>{ Ok(Self) }
}
pub struct ServiceClient;
impl ServiceClient {
    pub fn new(_connection:ServiceConnection)->ClientResult<Self>{Ok(Self)}
    pub async fn capabilities(&self)->ClientResult<Value>{Err(ClientError::Retryable)}
    pub async fn register_source(&self,_source:SourceKind,_key:&str)->ClientResult<String>{Err(ClientError::Retryable)}
    pub async fn submit_snapshot(&self,_input:&PendingSubmission)->ClientResult<SnapshotReceipt>{Err(ClientError::Retryable)}
    pub async fn get_job(&self,_id:&str)->ClientResult<Value>{Err(ClientError::Retryable)}
}
#[derive(Debug,Clone,PartialEq,Eq)]
pub enum EnqueueOutcome { Queued(String), Existing(String), Unchanged }
pub struct ClaimedUpload { pending:PendingSubmission }
impl ClaimedUpload { pub fn pending(&self)->&PendingSubmission{&self.pending} }
pub struct CollectorOutbox;
impl CollectorOutbox {
    pub fn open(_path:&Path)->ClientResult<Self>{Ok(Self)}
    pub fn enqueue(&self,_pending:&PendingSubmission)->ClientResult<EnqueueOutcome>{Err(ClientError::Storage)}
    pub fn next_for(&self,_instance:&str,_space:&str,_now:u64)->ClientResult<Option<ClaimedUpload>>{Ok(None)}
    pub fn record_receipt(&self,_claim:&ClaimedUpload,_receipt:&SnapshotReceipt)->ClientResult<()>{Err(ClientError::Storage)}
    pub fn record_failure(&self,_claim:&ClaimedUpload,_error:ClientError,_now:u64)->ClientResult<()>{Err(ClientError::Storage)}
    pub fn revision_for(&self,_instance:&str,_space:&str,_registration:&str,_upstream:&str)->ClientResult<u32>{Ok(0)}
}
