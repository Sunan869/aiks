use aiks_core::{
    config::{Config, ExternalProviderConfig},
    model::{NormalizedSession, SourceKind},
    providers::{
        native::NativeProvider, ProviderHealth, ProviderRegistry, SessionProvider, SessionSummary,
    },
    storage::{SourceSessionRepo, StateDb},
    sync::engine::SyncEngine,
};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Spy {
    source: SourceKind,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl SessionProvider for Spy {
    fn source(&self) -> SourceKind {
        self.source
    }
    fn parser_version(&self) -> &'static str {
        "spy-v1"
    }
    async fn discover_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }
    async fn load_session(&self, _: &SessionSummary) -> anyhow::Result<NormalizedSession> {
        anyhow::bail!("not used")
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Ok
    }
}
#[tokio::test]
async fn selected_source_does_not_touch_another_provider() {
    let a = Arc::new(AtomicUsize::new(0));
    let b = Arc::new(AtomicUsize::new(0));
    let registry = ProviderRegistry::new(vec![
        Box::new(Spy {
            source: SourceKind::Continue,
            calls: a.clone(),
        }),
        Box::new(Spy {
            source: SourceKind::QwenCode,
            calls: b.clone(),
        }),
    ]);
    let reports = registry.discover_selected(Some(SourceKind::Continue)).await;
    assert_eq!(reports.len(), 1);
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn removing_a_configured_root_does_not_mark_its_old_sessions_missing() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    std::fs::create_dir_all(a.join("sessions")).unwrap();
    std::fs::create_dir_all(b.join("sessions")).unwrap();
    let removed = a.join("sessions/removed.json");
    let retained = b.join("sessions/retained.json");
    for (id, path) in [("from-a", &removed), ("from-b", &retained)] {
        let value = json!({
            "sessionId": id,
            "title": id,
            "history": [{"message": {"role": "user", "content": "Synthetic question"}}]
        });
        std::fs::write(path, value.to_string()).unwrap();
    }
    let initial = NativeProvider::new(
        SourceKind::Continue,
        &ExternalProviderConfig {
            paths: vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    let discovered = initial.discover_sessions().await.unwrap();
    assert_eq!(discovered.len(), 2);
    let from_a = discovered
        .iter()
        .find(|s| s.title.as_deref() == Some("from-a"))
        .unwrap();
    let from_b = discovered
        .iter()
        .find(|s| s.title.as_deref() == Some("from-b"))
        .unwrap();
    // Persist the same canonical identities and paths as production sync. On
    // Windows canonical paths may use a verbatim prefix; invented raw paths
    // were not representative of records produced by these native adapters.
    assert_eq!(from_a.source_path, Some(removed.canonicalize().unwrap()));
    assert_eq!(from_b.source_path, Some(retained.canonicalize().unwrap()));
    let db = StateDb::open(&root.path().join("aiks.db")).unwrap();
    let repo = SourceSessionRepo::new(&db);
    for summary in &discovered {
        repo.upsert(
            "continue",
            &summary.external_session_id,
            summary.source_path.as_deref().and_then(|path| path.to_str()),
            None,
            None,
            summary.title.as_deref(),
            None,
            Some("old"),
            Some(initial.parser_version()),
        )
        .unwrap();
    }
    // A truly deleted file inside the still-configured root is missing. B is
    // deliberately no longer configured, but its real file and record survive.
    std::fs::remove_file(&removed).unwrap();
    let p = NativeProvider::new(
        SourceKind::Continue,
        &ExternalProviderConfig {
            path: a.to_string_lossy().into_owned(),
            ..Default::default()
        },
    )
    .unwrap();
    let registry = ProviderRegistry::new(vec![Box::new(p)]);
    let sync = SyncEngine::new(Arc::new(Config::default()));
    assert_eq!(sync.mark_missing_sessions(&db, &registry).await.unwrap(), 1);
    assert!(
        repo.find_by_source_and_id("continue", &from_a.external_session_id)
            .unwrap()
            .unwrap()
            .is_missing
    );
    assert!(
        !repo
            .find_by_source_and_id("continue", &from_b.external_session_id)
            .unwrap()
            .unwrap()
            .is_missing
    );
    assert!(retained.is_file());
}
#[tokio::test]
async fn damaged_and_disabled_sources_cannot_mark_missing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("sessions")).unwrap();
    std::fs::write(root.path().join("sessions/corrupt.json"), "{broken").unwrap();
    let db = StateDb::open(&root.path().join("aiks.db")).unwrap();
    let repo = SourceSessionRepo::new(&db);
    repo.upsert(
        "continue",
        "old",
        Some(root.path().join("sessions/old.json").to_str().unwrap()),
        None,
        None,
        None,
        None,
        Some("old"),
        Some("v1"),
    )
    .unwrap();
    let p = NativeProvider::new(
        SourceKind::Continue,
        &ExternalProviderConfig {
            path: root.path().to_string_lossy().into_owned(),
            ..Default::default()
        },
    )
    .unwrap();
    let sync = SyncEngine::new(Arc::new(Config::default()));
    assert_eq!(
        sync.mark_missing_sessions(&db, &ProviderRegistry::new(vec![Box::new(p)]))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sync.mark_missing_sessions(&db, &ProviderRegistry::new(Vec::new()))
            .await
            .unwrap(),
        0
    );
    assert!(
        !repo
            .find_by_source_and_id("continue", "old")
            .unwrap()
            .unwrap()
            .is_missing
    );
}
