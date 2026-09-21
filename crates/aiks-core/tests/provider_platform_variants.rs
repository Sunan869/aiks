use aiks_core::{config::ExternalProviderConfig, model::{ContentBlock, MessageRole, SourceKind}, providers::{local_io::{ReadLimits, ScopedReader}, native::NativeProvider, SessionProvider}};
use serde_json::json;
use std::path::Path;

#[tokio::test]
async fn each_cline_family_ui_fallback_keeps_user_assistant_and_thinking() {
    for source in [SourceKind::Cline,SourceKind::RooCode,SourceKind::KiloCode] {
        let root=tempfile::tempdir().unwrap(); let task=root.path().join("tasks/任务一"); std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("task_metadata.json"),json!({"title":"中文任务","cwd":"C:\\示例\\项目"}).to_string()).unwrap();
        let history=json!([{"ts":1000,"type":"say","say":"text","text":"这是原始问题"},{"ts":1100,"type":"say","say":"api_req_started","text":"not a conversation"},{"ts":1200,"type":"say","say":"text","text":"这是回答"},{"ts":1300,"type":"say","say":"reasoning","reasoning":"推理内容"},{"ts":1400,"type":"say","say":"future-block","text":"保留未知块"}]);
        std::fs::write(task.join("ui_messages.json"),history.to_string()).unwrap();
        let p=NativeProvider::new(source,&ExternalProviderConfig { path:root.path().to_string_lossy().into_owned(), ..Default::default() }).unwrap();
        let summary=p.discover_sessions().await.unwrap().remove(0);let session=p.load_session(&summary).await.unwrap();
        assert_eq!(session.title.as_deref(),Some("中文任务")); assert_eq!(session.project_name.as_deref(),Some("项目")); assert_eq!(session.messages.len(),4);
        assert_eq!(session.messages[0].role,MessageRole::User); assert_eq!(session.messages[1].role,MessageRole::Assistant);
        assert!(matches!(session.messages[2].blocks[0],ContentBlock::Thinking { .. })); assert!(matches!(session.messages[3].blocks[0],ContentBlock::Unknown { .. }));
    }
}

#[cfg(windows)]
#[test]
fn windows_junction_root_and_parent_cannot_escape_the_configured_store() {
    let root=tempfile::tempdir().unwrap();let outside=tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("private.json"),"{}").unwrap();
    let link=root.path().join("junction");
    let output=std::process::Command::new("cmd").args(["/C","mklink","/J"]).arg(&link).arg(outside.path()).output().unwrap();
    assert!(output.status.success(),"fixture junction creation failed: {}",String::from_utf8_lossy(&output.stderr));
    let reader=ScopedReader::new(root.path().to_path_buf(),ReadLimits::default()).unwrap();
    let root_rejected=ScopedReader::new(link.clone(),ReadLimits::default()).is_err();
    let parent_rejected=reader.read_json(Path::new("junction/private.json")).is_err();
    std::fs::remove_dir(&link).unwrap();
    assert!(root_rejected && parent_rejected); assert_eq!(std::fs::read_to_string(outside.path().join("private.json")).unwrap(),"{}");
}

#[cfg(not(windows))]
#[test]
fn explicit_root_relative_io_stays_platform_independent() {
    let root=tempfile::tempdir().unwrap();std::fs::write(root.path().join("会话.json"),"{}").unwrap();
    let io=ScopedReader::new(root.path().to_path_buf(),ReadLimits::default()).unwrap();
    assert!(io.read_json(Path::new("会话.json")).is_ok());
}
