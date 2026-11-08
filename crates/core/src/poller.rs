//! One look at a server, and how soon to look again. "Unreachable" and "auth failed" are their
//! own states: not knowing is never reported as "0 running".

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::client::{Client, Error, RunFilter, STATUS_FAILED, STATUS_QUEUED, STATUS_RUNNING};
use crate::models::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "detail")]
pub enum Connection {
    Ok,
    Unreachable(String),
    AuthFailed,
    Error(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub connection: Connection,
    pub version: Option<String>,
    pub running: Vec<DagRun>,
    pub queued: Vec<DagRun>,
    pub failed: Vec<DagRun>,
    pub queues: Vec<Queue>,
    pub workers: Vec<Worker>,
    pub taken_at: DateTime<Utc>,
}

impl Snapshot {
    fn empty(connection: Connection, taken_at: DateTime<Utc>) -> Self {
        Self {
            connection,
            version: None,
            running: vec![],
            queued: vec![],
            failed: vec![],
            queues: vec![],
            workers: vec![],
            taken_at,
        }
    }
}

fn connection_for(err: &Error) -> Connection {
    match err {
        Error::Unreachable(m) => Connection::Unreachable(m.clone()),
        Error::AuthFailed => Connection::AuthFailed,
        other => Connection::Error(other.to_string()),
    }
}

/// `failed_since` bounds the failure list: normally the last day.
pub async fn fetch_snapshot(client: &Client, failed_since: DateTime<Utc>, now: DateTime<Utc>) -> Snapshot {
    // Reads need health to answer first; a server that fails here is reported as such.
    let health = match client.health().await {
        Ok(h) => h,
        Err(e) => return Snapshot::empty(connection_for(&e), now),
    };
    let running = RunFilter { statuses: &[STATUS_RUNNING], limit: Some(100), ..Default::default() };
    let queued = RunFilter { statuses: &[STATUS_QUEUED], limit: Some(100), ..Default::default() };
    let failed = RunFilter {
        statuses: &[STATUS_FAILED],
        from: Some(failed_since),
        limit: Some(50),
        ..Default::default()
    };
    let (r, q, f) = (client.runs(&running).await, client.runs(&queued).await, client.runs(&failed).await);
    let (running, queued, failed) = match (r, q, f) {
        (Ok(r), Ok(q), Ok(f)) => (r, q, f),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
            return Snapshot::empty(connection_for(&e), now)
        }
    };
    // Queues and workers are supplementary: a server without them still shows its runs.
    let queues = client.queues().await.unwrap_or_default();
    let workers = client.workers().await.unwrap_or_default();
    Snapshot {
        connection: Connection::Ok,
        version: health.version,
        running,
        queued,
        failed,
        queues,
        workers,
        taken_at: now,
    }
}

pub fn poll_interval(popover_open: bool, runs_active: bool) -> Duration {
    Duration::from_secs(match (popover_open, runs_active) {
        (true, _) => 5,
        (false, true) => 15,
        (false, false) => 60,
    })
}

/// Doubles from 5 s to at most 5 minutes while a server keeps failing.
#[derive(Debug, Default, Clone)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    pub fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }

    pub fn reset(&mut self) {
        self.failures = 0;
    }

    pub fn delay(&self) -> Option<Duration> {
        if self.failures == 0 {
            return None;
        }
        let exp = (self.failures - 1).min(16);
        Some(Duration::from_secs((5u64 << exp).min(300)))
    }
}
