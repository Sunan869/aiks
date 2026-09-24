//! Explicit local capture and durable delivery. Providers stay on the client.
use super::{
    valid_id, ClientError, ClientResult, CollectorOutbox, EnqueueOutcome, PendingSubmission,
    ServiceClient,
};
use aiks_core::{
    model::NormalizedSession,
    providers::{
        local_io::{ReadLimits, ScopedReader},
        SessionProvider,
    },
    service::{validate_submission, SnapshotReceipt, SnapshotSubmission},
    util::sanitizer::default_sanitizer,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub struct CollectionPolicy {
    pub source_key: String,
    pub include_ids: Option<Vec<String>>,
    pub exclude_ids: Vec<String>,
    pub redact_secrets: bool,
    pub max_sessions: usize,
}
impl Default for CollectionPolicy {
    fn default() -> Self {
        Self {
            source_key: "default".into(),
            include_ids: None,
            exclude_ids: Vec::new(),
            redact_secrets: true,
            max_sessions: 100,
        }
    }
}
#[derive(Default, Serialize)]
pub struct CollectionReport {
    pub discovered: usize,
    pub queued: usize,
    pub unchanged: usize,
    pub deferred: usize,
    pub excluded: usize,
    pub failed: usize,
    pub complete: bool,
}

pub async fn collect_provider(
    provider: &dyn SessionProvider,
    client: &ServiceClient,
    outbox: Arc<CollectorOutbox>,
    policy: &CollectionPolicy,
) -> ClientResult<CollectionReport> {
    if !valid_id(&policy.source_key)
        || policy.source_key.len() > 128
        || policy.max_sessions == 0
        || policy.max_sessions > 1000
        || policy
            .include_ids
            .as_ref()
            .is_some_and(|v| v.len() > 1000 || v.iter().any(|v| !valid_id(v)))
        || policy.exclude_ids.len() > 1000
        || policy.exclude_ids.iter().any(|v| !valid_id(v))
    {
        return Err(ClientError::InvalidInput);
    }
    let identity = client.connection();
    let target = identity.target_identity().clone();
    let instance = identity.instance_id().to_owned();
    let space = identity.space_id().to_owned();
    let source = provider.source();
    let scope =
        super::preferences::SourceScope::for_target(target.clone(), source, &policy.source_key)?;
    // Personal mode keeps the v1 key exactly; team mode adds company/user.
    // Registration, exclusion and scan keys must share the same identity boundary.
    let key = scope.key()?;
    let store = outbox.clone();
    let saved_exclusions = blocking(move || store.excluded(&scope)).await?;
    let scan_key = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                &key,
                &policy.include_ids,
                &policy.exclude_ids,
                &saved_exclusions
            ))
            .map_err(|_| ClientError::InvalidInput)?
        )
    );
    let db = outbox.clone();
    let lookup = key.clone();
    let scan_lookup = scan_key.clone();
    let (known, device, cursor) = blocking(move || {
        Ok((
            db.registration(&lookup)?,
            db.device_id()?,
            db.scan_cursor(&scan_lookup)?,
        ))
    })
    .await?;
    let registration = match known {
        Some(value) => value,
        None => {
            let value = client
                .register_source(source, &format!("{device}:{}", policy.source_key))
                .await?;
            let db = outbox.clone();
            let stored = value.clone();
            blocking(move || db.remember_registration(&key, &stored)).await?;
            value
        }
    };
    let discovery = provider
        .discover_report()
        .await
        .map_err(|_| ClientError::InvalidInput)?;
    let mut report = CollectionReport {
        discovered: discovery.sessions.len(),
        complete: discovery.complete,
        ..Default::default()
    };
    let mut sessions = Vec::new();
    for summary in discovery.sessions {
        let id = &summary.external_session_id;
        if saved_exclusions.contains(id)
            || policy.exclude_ids.contains(id)
            || policy.include_ids.as_ref().is_some_and(|v| !v.contains(id))
        {
            report.excluded += 1;
        } else {
            sessions.push(summary);
        }
    }
    sessions.sort_by(|a, b| a.external_session_id.cmp(&b.external_session_id));
    let mut start = cursor.as_ref().map_or(0, |id| {
        sessions.partition_point(|s| s.external_session_id <= *id)
    });
    if start == sessions.len() {
        start = 0;
    }
    let end = start
        .saturating_add(policy.max_sessions)
        .min(sessions.len());
    report.deferred = sessions.len() - end;
    if report.deferred > 0 {
        report.complete = false;
    }
    let next = if end < sessions.len() {
        Some(sessions[end - 1].external_session_id.clone())
    } else {
        None
    };
    for summary in sessions.into_iter().skip(start).take(policy.max_sessions) {
        let upstream = &summary.external_session_id;
        let mut session = match load_complete(provider, &summary).await {
            Ok(session) => session,
            Err(_) => {
                report.failed += 1;
                report.complete = false;
                continue;
            }
        };
        let db = outbox.clone();
        let (target_for_revision, r, u) = (target.clone(), registration.clone(), upstream.clone());
        let revision =
            blocking(move || db.revision_for_target(&target_for_revision, &r, &u)).await?;
        let mut input = SnapshotSubmission {
            api_version: 1,
            submission_id: uuid::Uuid::new_v4().to_string(),
            service_instance_id: instance.clone(),
            space_id: space.clone(),
            source_registration_id: registration.clone(),
            expected_revision: revision,
            complete: true,
            parser_version: provider.parser_version().into(),
            session: session.clone(),
        };
        if validate_submission(&input).is_err() {
            report.failed += 1;
            report.complete = false;
            continue;
        }
        session.source_path = None;
        session.project_path = None;
        session.metadata.clear();
        for message in &mut session.messages {
            message.metadata.clear();
        }
        if policy.redact_secrets && sanitize(&mut session).is_err() {
            report.failed += 1;
            report.complete = false;
            continue;
        }
        input.session = session;
        let pending = PendingSubmission::new(input)?;
        let db = outbox.clone();
        let target_for_enqueue = target.clone();
        match blocking(move || db.enqueue_for(&target_for_enqueue, &pending)).await {
            Ok(EnqueueOutcome::Queued(_)) => report.queued += 1,
            Ok(EnqueueOutcome::Existing(_) | EnqueueOutcome::Unchanged) => report.unchanged += 1,
            Err(ClientError::Busy) => {
                report.deferred += 1;
                report.complete = false;
            }
            Err(error) => return Err(error),
        }
    }
    // Persist only after the bounded batch. A crash earlier coalesces already
    // queued snapshots when replayed. A scan cursor never acknowledges uploads.
    blocking(move || outbox.save_scan_cursor(&scan_key, next.as_deref())).await?;
    Ok(report)
}
pub async fn deliver_one(
    client: &ServiceClient,
    outbox: Arc<CollectorOutbox>,
    now: u64,
) -> ClientResult<Option<SnapshotReceipt>> {
    let target = client.connection().target_identity().clone();
    let db = outbox.clone();
    let Some(claim) = blocking(move || db.next_for_target(&target, now)).await? else {
        return Ok(None);
    };
    match client.submit_snapshot(claim.pending()).await {
        Ok(receipt) => {
            let saved = receipt.clone();
            blocking(move || outbox.record_receipt(&claim, &saved)).await?;
            Ok(Some(receipt))
        }
        Err(error) => {
            blocking(move || outbox.record_failure(&claim, error, now)).await?;
            Err(error)
        }
    }
}
/// Shared with read-only preview so neither path trusts partial JSONL or a stale identity.
pub(super) async fn load_complete(
    provider: &dyn SessionProvider,
    summary: &aiks_core::providers::SessionSummary,
) -> ClientResult<NormalizedSession> {
    let path = summary.source_path.clone();
    let before = blocking(move || jsonl_guard(path.as_deref())).await?;
    let session = provider
        .load_session(summary)
        .await
        .map_err(|_| ClientError::InvalidInput)?;
    if session.source != provider.source()
        || session.external_session_id != summary.external_session_id
    {
        return Err(ClientError::InvalidInput);
    }
    if let Some((path, stamp)) = before {
        if !blocking(move || Ok(stamp_for(&path)? == stamp)).await? {
            return Err(ClientError::InvalidInput);
        }
    }
    Ok(session)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> ClientResult<T> + Send + 'static,
) -> ClientResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| ClientError::Storage)?
}
#[derive(PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
fn stamp_for(path: &Path) -> ClientResult<Stamp> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| ClientError::InvalidInput)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(ClientError::InvalidInput);
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(Stamp {
        length: meta.len(),
        modified: meta.modified().map_err(|_| ClientError::InvalidInput)?,
        #[cfg(unix)]
        device: meta.dev(),
        #[cfg(unix)]
        inode: meta.ino(),
    })
}
fn jsonl_guard(path: Option<&Path>) -> ClientResult<Option<(PathBuf, Stamp)>> {
    let Some(path) = path.filter(|p| p.extension().and_then(|v| v.to_str()) == Some("jsonl"))
    else {
        return Ok(None);
    };
    let parent = path.parent().ok_or(ClientError::InvalidInput)?;
    let io = ScopedReader::new(parent.to_path_buf(), ReadLimits::default())
        .map_err(|_| ClientError::InvalidInput)?;
    let relative = Path::new(path.file_name().ok_or(ClientError::InvalidInput)?);
    let checked = io
        .checked_path(relative)
        .map_err(|_| ClientError::InvalidInput)?;
    let before = stamp_for(&checked)?;
    let report = io
        .for_each_jsonl(relative, |_, _| Ok(()))
        .map_err(|_| ClientError::InvalidInput)?;
    if !report.complete || before != stamp_for(&checked)? {
        return Err(ClientError::InvalidInput);
    }
    Ok(Some((checked, before)))
}
pub(super) fn sanitize(session: &mut NormalizedSession) -> ClientResult<()> {
    let sanitizer = default_sanitizer();
    for value in [
        &mut session.title,
        &mut session.project_name,
        &mut session.model,
    ]
    .into_iter()
    .flatten()
    {
        *value = sanitizer.sanitize(value);
    }
    for message in &mut session.messages {
        if let Some(value) = &mut message.model {
            *value = sanitizer.sanitize(value);
        }
        for block in &mut message.blocks {
            let mut value = serde_json::to_value(&*block).map_err(|_| ClientError::InvalidInput)?;
            scrub(&mut value);
            *block = serde_json::from_value(value).map_err(|_| ClientError::InvalidInput)?;
        }
    }
    Ok(())
}
fn scrub(value: &mut Value) {
    match value {
        Value::String(text) => *text = default_sanitizer().sanitize(text),
        Value::Array(items) => {
            for value in items {
                scrub(value)
            }
        }
        Value::Object(items) => {
            for (key, value) in items {
                let key = key.to_ascii_lowercase().replace(['_', '-'], "");
                if matches!(
                    key.as_str(),
                    "apikey"
                        | "token"
                        | "secret"
                        | "password"
                        | "authorization"
                        | "accesstoken"
                        | "clientsecret"
                        | "privatekey"
                        | "secretkey"
                        | "accesskey"
                ) {
                    *value = Value::String("[REDACTED]".into());
                } else {
                    scrub(value)
                }
            }
        }
        _ => {}
    }
}
