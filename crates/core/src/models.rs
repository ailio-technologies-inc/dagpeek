//! The parts of Dagu's API Dagpeek reads. Labels the server adds later decode to `Unknown`
//! instead of failing the whole response.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

macro_rules! status {
    ($name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name { $($variant,)+ Unknown(String) }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                match s.as_str() { $($label => Self::$variant,)+ _ => Self::Unknown(s) }
            }
        }

        impl From<$name> for String {
            fn from(s: $name) -> String {
                match s { $($name::$variant => $label.to_string(),)+ $name::Unknown(o) => o }
            }
        }
    };
}

status!(RunStatus {
    NotStarted => "not_started",
    Running => "running",
    Failed => "failed",
    Aborted => "aborted",
    Succeeded => "succeeded",
    Queued => "queued",
    PartiallySucceeded => "partially_succeeded",
    Waiting => "waiting",
    Rejected => "rejected",
});

status!(NodeStatus {
    NotStarted => "not_started",
    Running => "running",
    Failed => "failed",
    Aborted => "aborted",
    Succeeded => "succeeded",
    Skipped => "skipped",
    PartiallySucceeded => "partially_succeeded",
    Waiting => "waiting",
    Rejected => "rejected",
    Retrying => "retrying",
});

impl NodeStatus {
    /// Whether the step will do no more work in this run.
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Aborted | Self::Succeeded | Self::Skipped
                | Self::PartiallySucceeded | Self::Rejected
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Retrying | Self::Waiting)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DagRun {
    pub name: String,
    pub dag_run_id: String,
    #[serde(rename = "statusLabel")]
    pub status: RunStatus,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub queued_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl DagRun {
    pub fn all_tags(&self) -> impl Iterator<Item = &String> {
        self.labels.iter().chain(self.tags.iter())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub step: Step,
    #[serde(rename = "statusLabel")]
    pub status: NodeStatus,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub retry_count: u32,
}

impl Node {
    pub fn duration_secs(&self) -> Option<f64> {
        let (a, b) = (self.started_at?, self.finished_at?);
        Some(((b - a).num_milliseconds() as f64 / 1000.0).max(0.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetails {
    pub name: String,
    pub dag_run_id: String,
    #[serde(rename = "statusLabel")]
    pub status: RunStatus,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "crate::time::lenient")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub name: String,
    #[serde(default)]
    pub max_concurrency: Option<u32>,
    #[serde(default)]
    pub queued_count: u32,
    #[serde(default)]
    pub running_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
    pub id: String,
    pub health_status: String,
    #[serde(default)]
    pub busy_pollers: u32,
    #[serde(default)]
    pub total_pollers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
}
