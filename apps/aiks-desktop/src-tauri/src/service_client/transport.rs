use std::{net::{IpAddr,Ipv4Addr,Ipv6Addr},sync::Arc,time::Duration};
use aiks_core::{model::SourceKind,service::SnapshotReceipt};
use reqwest::{Client,Method,StatusCode,Url};
use serde::{de::DeserializeOwned,Deserialize};
use serde_json::{json,Value};
use super::{valid_id,ClientError,ClientResult,PendingSubmission};

/// Credential stays in native memory, never in a queue, URL, Debug or webview DTO.
#[derive(Clone)]
pub struct ServiceConnection {
    base:Url,
    instance_id:String,
    space_id:String,
    credential:Arc<str>,
}
impl ServiceConnection {
    pub fn local(url:&str,instance:&str,space:&str,token:&str)->ClientResult<Self> {
        let base=Url::parse(url).map_err(|_|ClientError::InvalidInput)?;
        let ip=base.host_str().and_then(|s|s.trim_matches(['[',']']).parse::<IpAddr>().ok());
        let allowed=ip==Some(IpAddr::V4(Ipv4Addr::LOCALHOST)) || ip==Some(IpAddr::V6(Ipv6Addr::LOCALHOST));
        if !allowed || base.scheme()!="http" || base.port_or_known_default().is_none_or(|v|v==0)
            || !base.username().is_empty() || base.password().is_some()
            || base.query().is_some() || base.fragment().is_some() || base.path()!="/"
            || !valid_id(instance) || !valid_id(space)
            || token.len()!=64 || !token.bytes().all(|b|b.is_ascii_hexdigit()) {
            return Err(ClientError::InvalidInput);
        }
        Ok(Self {base,instance_id:instance.into(),space_id:space.into(),credential:Arc::from(token)})
    }
    pub fn instance_id(&self)->&str{&self.instance_id}
    pub fn space_id(&self)->&str{&self.space_id}
}

#[derive(Clone)]
pub struct ServiceClient { connection:ServiceConnection, client:Client }
impl ServiceClient {
    pub fn new(connection:ServiceConnection)->ClientResult<Self> {
        let client=Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3)).timeout(Duration::from_secs(30))
            .build().map_err(|_|ClientError::InvalidInput)?;
        Ok(Self {connection,client})
    }
    pub fn connection(&self)->&ServiceConnection{&self.connection}
    pub async fn capabilities(&self)->ClientResult<Value> {
        let value:Value=self.request(Method::GET,&["capabilities"],None).await?;
        if value["instance_id"].as_str()!=Some(self.connection.instance_id())
            || value["space_id"].as_str()!=Some(self.connection.space_id()) {
            return Err(ClientError::WrongInstance);
        }
        if value["api_version"]!=1 || value["mode"]!="personal" || value["team"]!=false {
            return Err(ClientError::InvalidResponse);
        }
        Ok(value)
    }
    pub async fn register_source(&self,source:SourceKind,key:&str)->ClientResult<String> {
        if !valid_id(key){return Err(ClientError::InvalidInput);}
        self.capabilities().await?;
        #[derive(Deserialize)] struct Registration{source_registration_id:String}
        let value=self.json_body(&json!({"source":source,"registration_key":key}))?;
        let response:Registration=self.request(Method::POST,&["source-registrations"],Some(&value)).await?;
        if !valid_id(&response.source_registration_id){return Err(ClientError::InvalidResponse);}
        Ok(response.source_registration_id)
    }
    pub async fn submit_snapshot(&self,input:&PendingSubmission)->ClientResult<SnapshotReceipt> {
        let request=input.submission();
        if request.service_instance_id!=self.connection.instance_id || request.space_id!=self.connection.space_id {
            return Err(ClientError::WrongInstance);
        }
        // No conversation bytes are sent until the endpoint proves the expected
        // instance at the application protocol boundary.
        self.capabilities().await?;
        let receipt:SnapshotReceipt=self.request(Method::POST,&["session-snapshots"],Some(&input.body)).await?;
        validate_receipt(input,&receipt)?;
        Ok(receipt)
    }
    pub async fn get_receipt(&self,id:&str)->ClientResult<SnapshotReceipt>{
        self.read(&["receipts",id]).await
    }
    pub async fn get_job(&self,id:&str)->ClientResult<Value>{self.read(&["jobs",id]).await}
    pub async fn sessions(&self)->ClientResult<Value>{self.read(&["sessions"]).await}
    pub async fn knowledge(&self,id:&str)->ClientResult<Value>{self.read(&["knowledge",id]).await}
    pub async fn search(&self,query:&str)->ClientResult<Value>{
        if query.trim().is_empty() || query.len()>16_384 {return Err(ClientError::InvalidInput);}
        self.capabilities().await?;
        let body=self.json_body(&json!({"query":query,"limit":30}))?;
        self.request(Method::POST,&["search"],Some(&body)).await
    }
    async fn read<T:DeserializeOwned>(&self,segments:&[&str])->ClientResult<T>{
        if segments.iter().any(|v|!valid_id(v)){return Err(ClientError::InvalidInput);}
        self.capabilities().await?;
        self.request(Method::GET,segments,None).await
    }
    fn json_body(&self,value:&impl serde::Serialize)->ClientResult<Vec<u8>>{
        serde_json::to_vec(value).map_err(|_|ClientError::InvalidInput)
    }
    async fn request<T:DeserializeOwned>(&self,method:Method,segments:&[&str],body:Option<&[u8]>)->ClientResult<T>{
        let mut url=self.connection.base.clone();
        {
            let mut path=url.path_segments_mut().map_err(|_|ClientError::InvalidInput)?;
            path.clear().push("api").push("v1");
            for segment in segments {path.push(segment);}
        }
        let mut request=self.client.request(method,url).bearer_auth(self.connection.credential.as_ref())
            .header("X-AIKS-Instance-ID",self.connection.instance_id());
        if let Some(body)=body {request=request.header("Content-Type","application/json").body(body.to_vec());}
        let mut response=request.send().await.map_err(|_|ClientError::Retryable)?;
        let status=response.status();
        if !status.is_success(){return Err(classify_status(status));}
        let mut bytes=Vec::new();
        // Includes the JSON envelope around the largest accepted snapshot.
        const RESPONSE_LIMIT:usize=17*1024*1024;
        while let Some(chunk)=response.chunk().await.map_err(|_|ClientError::Retryable)?{
            if bytes.len().saturating_add(chunk.len())>RESPONSE_LIMIT{return Err(ClientError::TooLarge);}
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_|ClientError::InvalidResponse)
    }
}
fn classify_status(status:StatusCode)->ClientError{
    match status {
        StatusCode::UNAUTHORIZED|StatusCode::FORBIDDEN=>ClientError::Unauthorized,
        StatusCode::NOT_FOUND=>ClientError::NotFound,
        StatusCode::CONFLICT=>ClientError::Conflict,
        StatusCode::PAYLOAD_TOO_LARGE=>ClientError::TooLarge,
        StatusCode::REQUEST_TIMEOUT|StatusCode::TOO_MANY_REQUESTS=>ClientError::Retryable,
        status if status.is_server_error()=>ClientError::Retryable,
        _=>ClientError::InvalidResponse,
    }
}
pub(super) fn validate_receipt(input:&PendingSubmission,receipt:&SnapshotReceipt)->ClientResult<()> {
    let expected=input.submission.expected_revision;
    if receipt.state!="accepted" || receipt.revision==0
        || !(receipt.revision==expected || expected.checked_add(1)==Some(receipt.revision))
        || [&receipt.receipt_id,&receipt.session_id,&receipt.snapshot_id,&receipt.job_id,&receipt.pipeline_run_id]
            .iter().any(|s|!valid_id(s)) {
        return Err(ClientError::InvalidResponse);
    }
    Ok(())
}
