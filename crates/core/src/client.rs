//! Read-only Dagu API client. Runs in Rust, never in the webview, so any server works without
//! CORS or `http://` exceptions and secrets stay out of JavaScript.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize};
use urlencoding::encode;

use crate::models::*;

#[derive(Debug, Clone)]
pub enum Auth {
    None,
    Basic { user: String, password: String },
    Bearer(String),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("server unreachable: {0}")]
    Unreachable(String),
    #[error("the server refused these credentials")]
    AuthFailed,
    #[error("server answered {status} for {path}")]
    Http { status: u16, path: String },
    #[error("could not read the server's response to {path}: {message}")]
    Decode { path: String, message: String },
}

pub struct Client {
    base: String,
    auth: Auth,
    http: reqwest::Client,
}

/// Dagu's numeric run status codes, for the `status` filter.
pub const STATUS_RUNNING: u8 = 1;
pub const STATUS_FAILED: u8 = 2;
pub const STATUS_SUCCESS: u8 = 4;
pub const STATUS_QUEUED: u8 = 5;

#[derive(Debug, Default, Clone)]
pub struct RunFilter<'a> {
    pub name: Option<&'a str>,
    pub statuses: &'a [u8],
    pub from: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

impl Client {
    pub fn new(base_url: &str, auth: Auth) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent(concat!("Dagpeek/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the HTTP client configuration is static");
        Self { base: base_url.trim_end_matches('/').to_string(), auth, http }
    }

    /// The link to a run in Dagu's own web UI.
    pub fn run_url(&self, name: &str, id: &str) -> String {
        format!("{}/dag-runs/{}/{}", self.base, encode(name), encode(id))
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let mut req = self.http.get(format!("{}/api/v1{}", self.base, path));
        req = match &self.auth {
            Auth::None => req,
            Auth::Basic { user, password } => req.basic_auth(user, Some(password)),
            Auth::Bearer(token) => req.bearer_auth(token),
        };
        let resp = req.send().await.map_err(|e| Error::Unreachable(e.to_string()))?;
        match resp.status().as_u16() {
            200..=299 => {}
            401 | 403 => return Err(Error::AuthFailed),
            status => return Err(Error::Http { status, path: path.to_string() }),
        }
        let body = resp.bytes().await.map_err(|e| Error::Unreachable(e.to_string()))?;
        serde_json::from_slice(&body)
            .map_err(|e| Error::Decode { path: path.to_string(), message: e.to_string() })
    }

    pub async fn health(&self) -> Result<Health, Error> {
        self.get("/health").await
    }

    pub async fn runs(&self, filter: &RunFilter<'_>) -> Result<Vec<DagRun>, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            #[serde(default)]
            dag_runs: Option<Vec<DagRun>>,
        }
        let mut query = vec![format!("limit={}", filter.limit.unwrap_or(100).min(500))];
        if let Some(name) = filter.name {
            query.push(format!("name={}", encode(name)));
        }
        query.extend(filter.statuses.iter().map(|s| format!("status={s}")));
        if let Some(from) = filter.from {
            query.push(format!("fromDate={}", from.timestamp()));
        }
        let page: Page = self.get(&format!("/dag-runs?{}", query.join("&"))).await?;
        Ok(page.dag_runs.unwrap_or_default())
    }

    pub async fn run_details(&self, name: &str, id: &str) -> Result<RunDetails, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Resp {
            dag_run_details: RunDetails,
        }
        let path = format!("/dag-runs/{}/{}", encode(name), encode(id));
        Ok(self.get::<Resp>(&path).await?.dag_run_details)
    }

    pub async fn queues(&self) -> Result<Vec<Queue>, Error> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            queues: Option<Vec<Queue>>,
        }
        Ok(self.get::<Resp>("/queues").await?.queues.unwrap_or_default())
    }

    pub async fn queued_items(&self, queue: &str) -> Result<Vec<DagRun>, Error> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            items: Option<Vec<DagRun>>,
        }
        let path = format!("/queues/{}/items?limit=100", encode(queue));
        Ok(self.get::<Resp>(&path).await?.items.unwrap_or_default())
    }

    pub async fn workers(&self) -> Result<Vec<Worker>, Error> {
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            workers: Option<Vec<Worker>>,
        }
        Ok(self.get::<Resp>("/workers").await?.workers.unwrap_or_default())
    }
}
