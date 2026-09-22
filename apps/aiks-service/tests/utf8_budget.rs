use serde_json::Value;
mod support;
use support::RunningService;

#[tokio::test]
async fn draft_content_budget_is_measured_in_utf8_bytes_not_code_points() {
    let s = RunningService::start().await;
    let receipt = s.ingest_ready("Synthetic source").await;
    s.seed_knowledge(&receipt, "draft", None);
    let db = rusqlite::Connection::open(&s.path).unwrap();
    for content in ["a".repeat(1_048_577), "界".repeat(349_526)] {
        assert!(content.len() > 1024 * 1024);
        db.execute("UPDATE knowledge_item SET content=?1 WHERE id='draft'", [content]).unwrap();
        let response = s.auth(s.client.get(format!("{}/api/v1/knowledge/draft", s.base))).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["content_state"], "draft_too_large");
        assert!(value["content"].is_null());
    }
    drop(db);
    s.stop().await;
}
