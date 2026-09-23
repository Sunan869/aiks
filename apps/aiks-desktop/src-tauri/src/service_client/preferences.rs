//! Client-only preferences. They never authorize a server resource or open StateDb.
use super::{valid_id, ClientError, ClientResult, CollectorOutbox};
use aiks_core::SourceKind;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Onboarding {
    #[default]
    Unseen,
    Skipped,
    Completed,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UiPreferences {
    pub onboarding: Onboarding,
    pub selected_sources: Vec<String>,
}
/// Bound to the verified service identity, destination space and configured roots.
#[derive(Clone)]
pub struct SourceScope {
    pub instance: String,
    pub space: String,
    pub source: SourceKind,
    pub source_key: String,
}
impl SourceScope {
    pub fn new(
        instance: &str,
        space: &str,
        source: SourceKind,
        source_key: &str,
    ) -> ClientResult<Self> {
        if !valid_id(instance)
            || !valid_id(space)
            || !valid_id(source_key)
            || source_key.len() > 128
        {
            return Err(ClientError::InvalidInput);
        }
        Ok(Self {
            instance: instance.into(),
            space: space.into(),
            source,
            source_key: source_key.into(),
        })
    }
    pub fn key(&self) -> ClientResult<String> {
        serde_json::to_string(&(&self.instance, &self.space, self.source, &self.source_key))
            .map_err(|_| ClientError::InvalidInput)
    }
}
#[derive(Debug, Serialize)]
pub struct ExclusionChange {
    pub excluded_count: usize,
    pub paused: usize,
    pub already_received: usize,
}

impl CollectorOutbox {
    pub fn ui_preferences(&self) -> ClientResult<UiPreferences> {
        let conn = self.conn()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM collector_meta WHERE key='ui/preferences-v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|text| serde_json::from_str(&text).map_err(|_| ClientError::Storage))
            .unwrap_or_else(|| Ok(UiPreferences::default()))
    }
    pub fn finish_onboarding(&self, state: Onboarding) -> ClientResult<UiPreferences> {
        if state == Onboarding::Unseen {
            return Err(ClientError::InvalidInput);
        }
        self.change_ui(|prefs| prefs.onboarding = state)
    }
    pub fn save_selected_sources(&self, sources: Vec<String>) -> ClientResult<UiPreferences> {
        if sources.len() > 16
            || sources
                .iter()
                .any(|s| SourceKind::from_str(s).is_none_or(|kind| kind.as_str() != s))
        {
            return Err(ClientError::InvalidInput);
        }
        let mut sources = sources;
        sources.sort();
        sources.dedup();
        self.change_ui(|prefs| prefs.selected_sources = sources)
    }
    fn change_ui(&self, change: impl FnOnce(&mut UiPreferences)) -> ClientResult<UiPreferences> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value: Option<String> = tx
            .query_row(
                "SELECT value FROM collector_meta WHERE key='ui/preferences-v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let mut prefs = match value {
            Some(text) => serde_json::from_str(&text).map_err(|_| ClientError::Storage)?,
            None => UiPreferences::default(),
        };
        change(&mut prefs);
        let value = serde_json::to_string(&prefs).map_err(|_| ClientError::Storage)?;
        tx.execute(
            "INSERT INTO collector_meta(key,value) VALUES('ui/preferences-v1',?1)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [value],
        )?;
        tx.commit()?;
        Ok(prefs)
    }
    pub fn excluded(&self, scope: &SourceScope) -> ClientResult<Vec<String>> {
        let key = format!("exclusions/{}", scope.key()?);
        let value: Option<String> = self
            .conn()?
            .query_row(
                "SELECT value FROM collector_meta WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        decode_rules(value)
    }
    /// UI changes are applied atomically with pausing UNSENT envelopes. An
    /// in-flight request may already have been accepted, so refuse that race.
    /// Callers also serialize this operation with the collector/delivery gate.
    pub fn set_excluded(
        &self,
        scope: &SourceScope,
        ids: &[String],
        excluded: bool,
    ) -> ClientResult<ExclusionChange> {
        if ids.is_empty() || ids.len() > 1000 || ids.iter().any(|s| !valid_id(s)) {
            return Err(ClientError::InvalidInput);
        }
        let key = scope.key()?;
        let rules_key = format!("exclusions/{key}");
        let registration_key = format!("registration/{key}");
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let text: Option<String> = tx
            .query_row(
                "SELECT value FROM collector_meta WHERE key=?1",
                [&rules_key],
                |r| r.get(0),
            )
            .optional()?;
        let mut rules = decode_rules(text)?;
        let registration: Option<String> = tx
            .query_row(
                "SELECT value FROM collector_meta WHERE key=?1",
                [registration_key],
                |r| r.get(0),
            )
            .optional()?;
        let mut result = ExclusionChange {
            excluded_count: 0,
            paused: 0,
            already_received: 0,
        };
        let ids = ids.iter().collect::<std::collections::BTreeSet<_>>();
        for id in ids {
            if excluded {
                if !rules.contains(id) {
                    rules.push(id.clone());
                }
            } else {
                rules.retain(|r| r != id);
            }
            if let Some(registration) = &registration {
                let inflight: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM collector_upload
                    WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4 AND state='inflight')",
                    params![scope.instance, scope.space, registration, id], |r| r.get(0))?;
                if inflight {
                    return Err(ClientError::Busy);
                }
                let received: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM collector_cursor
                    WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4",
                    params![scope.instance, scope.space, registration, id],
                    |r| r.get(0),
                )?;
                result.already_received += received as usize;
                if excluded {
                    result.paused += tx.execute("UPDATE collector_upload SET state='blocked',error_code='excluded',due_ms=0
                        WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4 AND state='pending'",
                        params![scope.instance, scope.space, registration, id])?;
                } else {
                    tx.execute("UPDATE collector_upload SET state='pending',error_code=NULL,due_ms=0
                        WHERE instance_id=?1 AND space_id=?2 AND registration_id=?3 AND upstream_id=?4 AND state='blocked' AND error_code='excluded'",
                        params![scope.instance, scope.space, registration, id])?;
                }
            }
        }
        rules.sort();
        rules.dedup();
        if rules.len() > 1000 {
            return Err(ClientError::TooLarge);
        }
        result.excluded_count = rules.len();
        let text = serde_json::to_string(&rules).map_err(|_| ClientError::Storage)?;
        tx.execute("INSERT INTO collector_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![rules_key,text])?;
        tx.commit()?;
        Ok(result)
    }
}
fn decode_rules(value: Option<String>) -> ClientResult<Vec<String>> {
    let rules: Vec<String> = match value {
        Some(value) if value.len() <= 1024 * 1024 => {
            serde_json::from_str(&value).map_err(|_| ClientError::Storage)?
        }
        Some(_) => return Err(ClientError::Storage),
        None => vec![],
    };
    if rules.len() > 1000 || rules.iter().any(|id| !valid_id(id)) {
        return Err(ClientError::Storage);
    }
    Ok(rules)
}
