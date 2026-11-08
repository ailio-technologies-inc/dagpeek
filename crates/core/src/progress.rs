//! How far a run has got. An ETA is only shown when history supports one: fewer than
//! `MIN_RUNS` past successes means no ETA, never an invented one.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::models::*;

pub const MIN_RUNS: usize = 3;
pub const MAX_RUNS: usize = 10;

/// Median duration of each step over a DAG's recent successful runs.
#[derive(Debug, Clone, Default)]
pub struct History {
    pub runs: usize,
    pub medians: HashMap<String, f64>,
}

impl History {
    /// `details` should be newest first. Only the newest `MAX_RUNS` successes count.
    pub fn from_runs(details: &[RunDetails]) -> Self {
        let used: Vec<_> = details
            .iter()
            .filter(|d| d.status == RunStatus::Succeeded)
            .take(MAX_RUNS)
            .collect();
        let mut samples: HashMap<String, Vec<f64>> = HashMap::new();
        for run in &used {
            for node in &run.nodes {
                if let Some(secs) = node.duration_secs() {
                    samples.entry(node.step.name.clone()).or_default().push(secs);
                }
            }
        }
        let medians = samples.into_iter().map(|(k, v)| (k, median(v))).collect();
        Self { runs: used.len(), medians }
    }

    fn usable(&self) -> bool {
        self.runs >= MIN_RUNS
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let mid = v.len() / 2;
    if v.len() % 2 == 1 { v[mid] } else { (v[mid - 1] + v[mid]) / 2.0 }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub current_step: Option<String>,
    /// 0.0 to 1.0. `None` draws an indeterminate bar.
    pub fraction: Option<f64>,
    pub eta_secs: Option<u64>,
}

pub fn compute(run: &RunDetails, history: Option<&History>, now: DateTime<Utc>) -> Progress {
    let total = run.nodes.len();
    let done = run.nodes.iter().filter(|n| n.status.is_finished()).count();
    let current_step =
        run.nodes.iter().find(|n| n.status.is_active()).map(|n| n.step.name.clone());
    let history = history.filter(|h| h.usable());

    let weights = history.and_then(|h| {
        run.nodes
            .iter()
            .map(|n| h.medians.get(&n.step.name).copied())
            .collect::<Option<Vec<f64>>>()
    });

    let elapsed = |n: &Node| {
        n.started_at.map(|s| ((now - s).num_milliseconds() as f64 / 1000.0).max(0.0)).unwrap_or(0.0)
    };

    let (fraction, eta_secs) = match &weights {
        Some(w) if w.iter().sum::<f64>() > 0.0 => {
            let sum: f64 = w.iter().sum();
            let mut spent = 0.0;
            let mut remaining = 0.0;
            for (node, &median) in run.nodes.iter().zip(w) {
                if node.status.is_finished() {
                    spent += median;
                } else if node.status.is_active() {
                    let in_step = elapsed(node).min(median);
                    spent += in_step;
                    remaining += median - in_step;
                } else {
                    remaining += median;
                }
            }
            (Some((spent / sum).clamp(0.0, 1.0)), Some(remaining.round() as u64))
        }
        _ if total <= 1 && done == 0 => (None, None),
        _ if total == 0 => (None, None),
        _ => (Some(done as f64 / total as f64), None),
    };

    Progress { done, total, current_step, fraction, eta_secs }
}
