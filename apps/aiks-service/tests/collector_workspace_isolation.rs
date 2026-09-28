use aiks_core::model::SourceKind;
use aiks_service::{ServiceConfig, ServiceRuntime};

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn verified_collector_workspaces_do_not_share_session_namespace() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("state.db");
    let config = ServiceConfig::personal(database);
    let runtime = ServiceRuntime::open(config.runtime_config()).await.unwrap();
    let instance = runtime.context().instance_id().to_owned();

    let alice = runtime
        .collector_context("wk-principal-101", "wk-space-101")
        .unwrap();
    let bob = runtime
        .collector_context("wk-principal-202", "wk-space-202")
        .unwrap();

    let alice_registration = runtime
        .register_source_for(
            &alice,
            SourceKind::Continue,
            "same-desktop-registration".into(),
        )
        .await
        .unwrap();
    let bob_registration = runtime
        .register_source_for(
            &bob,
            SourceKind::Continue,
            "same-desktop-registration".into(),
        )
        .await
        .unwrap();

    assert_ne!(alice_registration, bob_registration);

    let alice_input = support::fixture::submission(
        alice.space_id(),
        &instance,
        &alice_registration,
        "same-submission",
        0,
        "ALICE_PRIVATE_SESSION",
    );
    let bob_input = support::fixture::submission(
        bob.space_id(),
        &instance,
        &bob_registration,
        "same-submission",
        0,
        "BOB_PRIVATE_SESSION",
    );

    let (alice_receipt, _) = runtime.accept_for(&alice, alice_input).await.unwrap();
    let (bob_receipt, _) = runtime.accept_for(&bob, bob_input).await.unwrap();

    assert_ne!(alice_receipt.session_id, bob_receipt.session_id);

    let alice_view = runtime
        .session_for(&alice, alice_receipt.session_id.clone())
        .await
        .unwrap();
    let bob_view = runtime
        .session_for(&bob, bob_receipt.session_id.clone())
        .await
        .unwrap();

    assert!(alice_view.to_string().contains("ALICE_PRIVATE_SESSION"));
    assert!(!alice_view.to_string().contains("BOB_PRIVATE_SESSION"));
    assert!(bob_view.to_string().contains("BOB_PRIVATE_SESSION"));
    assert!(!bob_view.to_string().contains("ALICE_PRIVATE_SESSION"));

    assert!(runtime
        .session_for(&alice, bob_receipt.session_id)
        .await
        .is_err());
    assert!(runtime
        .session_for(&bob, alice_receipt.session_id)
        .await
        .is_err());
}
