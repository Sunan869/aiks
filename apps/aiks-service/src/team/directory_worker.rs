//! One cancellable directory refresh task, with coalesced manual refreshes.
//! Only complete snapshots for the configured scope reach Core publication.
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aiks_core::team::{IdentityProvider, TeamError, TeamStore};
use serde::Serialize;
use tokio::{
    sync::{watch, Mutex, Notify},
    task::JoinHandle,
};

const REFRESH_BUDGET: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub struct DirectoryPolicy {
    pub scope: Vec<String>,
    pub refresh_seconds: u64,
    pub max_stale_seconds: u64,
}

impl DirectoryPolicy {
    pub fn validate(&self) -> Result<(), TeamError> {
        let mut seen = HashSet::new();
        if self.scope.is_empty()
            || self.scope.len() > 100
            || !(60..=900).contains(&self.refresh_seconds)
            || !(self.refresh_seconds..=3600).contains(&self.max_stale_seconds)
            || self.scope.iter().any(|id| {
                id.is_empty()
                    || id.len() > 512
                    || id.trim() != id
                    || id.chars().any(char::is_control)
                    || !seen.insert(id)
            })
        {
            return Err(TeamError::ConfigInvalid);
        }
        Ok(())
    }
}

#[derive(Clone, Default, Serialize)]
pub struct DirectoryStatus {
    pub attempt: u64,
    pub running: bool,
    pub stopped: bool,
    pub generation: u64,
    pub last_success_at: Option<u64>,
    pub error_code: Option<&'static str>,
}

pub struct DirectoryWorker {
    refresh: Arc<Notify>,
    stop: watch::Sender<bool>,
    state: watch::Sender<DirectoryStatus>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl DirectoryWorker {
    pub fn start(
        store: Arc<TeamStore>,
        provider: Arc<dyn IdentityProvider>,
        policy: DirectoryPolicy,
    ) -> Result<Self, TeamError> {
        policy.validate()?;
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| TeamError::Unavailable)?;
        let refresh = Arc::new(Notify::new());
        let (stop, stopped) = watch::channel(false);
        let (state, _) = watch::channel(DirectoryStatus::default());
        let task = runtime.spawn(run(store, provider, policy, refresh.clone(), stopped, state.clone()));
        Ok(Self {
            refresh,
            stop,
            state,
            task: Mutex::new(Some(task)),
        })
    }

    pub fn request_refresh(&self) {
        if !*self.stop.borrow() {
            // Notify stores at most one permit; repeated clicks cannot queue
            // unbounded snapshots or create simultaneous upstream requests.
            self.refresh.notify_one();
        }
    }

    pub fn status(&self) -> DirectoryStatus {
        self.state.borrow().clone()
    }

    pub async fn shutdown(&self, grace: Duration) -> Result<(), TeamError> {
        self.stop.send_replace(true);
        let mut task = self.task.lock().await;
        if let Some(handle) = task.as_mut() {
            // A timeout retains the handle so another shutdown can join it.
            // Dropping/aborting a blocking SQLite write would not roll it back.
            tokio::time::timeout(grace, handle)
                .await
                .map_err(|_| TeamError::Unavailable)?
                .map_err(|_| TeamError::Unavailable)?;
            *task = None;
        }
        Ok(())
    }
}

impl Drop for DirectoryWorker {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

async fn cancelled(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

fn now() -> Result<u64, TeamError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| TeamError::Unavailable)
}

async fn run(
    store: Arc<TeamStore>,
    provider: Arc<dyn IdentityProvider>,
    policy: DirectoryPolicy,
    refresh: Arc<Notify>,
    mut stop: watch::Receiver<bool>,
    state: watch::Sender<DirectoryStatus>,
) {
    loop {
        if *stop.borrow() {
            break;
        }
        state.send_modify(|status| {
            status.attempt = status.attempt.saturating_add(1);
            status.running = true;
        });
        let reply = tokio::select! {
            biased;
            _ = cancelled(&mut stop) => break,
            reply = tokio::time::timeout(REFRESH_BUDGET, provider.directory(&policy.scope)) => {
                reply.unwrap_or(Err(TeamError::Unavailable))
            }
        };
        if *stop.borrow() {
            break;
        }
        let store = store.clone();
        let refresh_policy = policy.clone();
        let result = tokio::task::spawn_blocking(move || {
            let timestamp = now()?;
            let published = reply.and_then(|snapshot| {
                let mut expected = refresh_policy.scope.clone();
                let mut observed = snapshot.scope.clone();
                expected.sort();
                observed.sort();
                if expected != observed
                    || !timestamp.checked_sub(snapshot.observed_at)
                        .is_some_and(|age| age <= refresh_policy.max_stale_seconds)
                {
                    return Err(TeamError::InvalidInput);
                }
                store.publish_directory(snapshot, timestamp)
            });
            match published {
                Ok(generation) => Ok((generation, timestamp)),
                Err(error) => {
                    // Do not alter the last successful snapshot/freshness when
                    // an upstream request or atomic publication fails.
                    store.record_directory_refresh_failure(timestamp)?;
                    Err(if error == TeamError::Storage {
                        TeamError::Storage
                    } else {
                        TeamError::DirectoryUnavailable
                    })
                }
            }
        })
        .await
        .unwrap_or(Err(TeamError::Storage));
        state.send_modify(|status| {
            status.running = false;
            match result {
                Ok((generation, timestamp)) => {
                    status.generation = generation;
                    status.last_success_at = Some(timestamp);
                    status.error_code = None;
                }
                Err(TeamError::Storage) => status.error_code = Some("directory_storage_failed"),
                Err(_) => status.error_code = Some("directory_refresh_failed"),
            }
        });
        tokio::select! {
            biased;
            _ = cancelled(&mut stop) => break,
            _ = refresh.notified() => {},
            _ = tokio::time::sleep(Duration::from_secs(policy.refresh_seconds)) => {},
        }
    }
    state.send_modify(|status| {
        status.running = false;
        status.stopped = true;
    });
}
