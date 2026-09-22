//! Small, typed native actions; no arbitrary endpoint, SQL, filesystem or process proxy.
use std::sync::Arc;
use aiks_core::config::BackendMode;
use serde_json::{json,Value};
use tauri::{AppHandle,Manager,Webview};
use crate::service_desktop::ServiceDesktop;
use aiks_core::service::SnapshotReceipt;

fn trusted(view:&Webview)->Result<(),String>{
    let url=view.url().map_err(|_|"untrusted_shell")?;
    let packaged=(url.scheme()=="tauri"&&url.host_str()==Some("localhost"))
        || (url.scheme()=="http"&&url.host_str()==Some("tauri.localhost"));
    let dev=cfg!(debug_assertions)&&url.scheme()=="http"&&url.host_str()==Some("localhost")&&url.port()==Some(1420);
    if view.label()!="control" || !(packaged||dev){return Err("untrusted_shell".into());}
    Ok(())
}
fn state(app:&AppHandle)->Result<Arc<ServiceDesktop>,String>{
    app.try_state::<Arc<ServiceDesktop>>().map(|s|s.inner().clone()).ok_or("service_not_ready".into())
}
#[tauri::command]
pub async fn service_status(app:AppHandle,webview:Webview)->Result<Value,String>{
    trusted(&webview)?;
    if let Ok(state)=state(&app){return Ok(state.status().await);}
    let mode=crate::lifecycle::selected_config().map_err(|_|"invalid_backend_configuration")?.backend.mode;
    Ok(json!({"mode":if mode==BackendMode::Legacy{"legacy"}else{"service_local"},"phase":"starting"}))
}
#[tauri::command]
pub async fn service_collect_selected(app:AppHandle,webview:Webview,sources:Vec<String>,exclude_ids:Vec<String>)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.collect(sources,exclude_ids).await
}
#[tauri::command]
pub async fn service_uploads(app:AppHandle,webview:Webview)->Result<Value,String>{
    trusted(&webview)?;let (client,outbox)=state(&app)?.connection().await?;
    let identity=client.connection().clone();
    tokio::task::spawn_blocking(move||{
        let rows=outbox.statuses(identity.instance_id(),identity.space_id()).map_err(|e|e.to_string())?;
        serde_json::to_value(rows).map_err(|_|"invalid_upload_state".into())
    }).await.map_err(|_|"collector_storage_unavailable".to_string())?
}
#[tauri::command]
pub async fn service_get_receipt(app:AppHandle,webview:Webview,id:String)->Result<SnapshotReceipt,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.get_receipt(&id).await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_get_job(app:AppHandle,webview:Webview,id:String)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.get_job(&id).await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_search(app:AppHandle,webview:Webview,query:String)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.search(&query).await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_sessions(app:AppHandle,webview:Webview)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.sessions().await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_session(app:AppHandle,webview:Webview,id:String)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.session(&id).await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_knowledge_list(app:AppHandle,webview:Webview)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.knowledge_list().await.map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn service_knowledge(app:AppHandle,webview:Webview,id:String)->Result<Value,String>{
    trusted(&webview)?;state(&app)?.connection().await?.0.knowledge(&id).await.map_err(|e|e.to_string())
}
