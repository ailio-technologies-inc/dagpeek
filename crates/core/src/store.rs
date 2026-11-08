//! The server list and settings on disk, shared by the app and the CLI. Secrets are never in
//! this file: they live in the macOS Keychain or Windows Credential Manager.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::client::Auth;
use crate::models::DagRun;

const KEYRING_SERVICE: &str = "com.ailio.dagpeek";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    #[default]
    None,
    Basic,
    Bearer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth: AuthKind,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub servers: Vec<Server>,
    pub active_server: Option<String>,
    pub hidden_names: Vec<String>,
    pub hidden_tags: Vec<String>,
    pub notify_failure: bool,
    pub notify_completion: bool,
    /// Failures finished before this time have been dismissed.
    pub failures_seen_at: Option<DateTime<Utc>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            servers: vec![],
            active_server: None,
            hidden_names: vec![],
            hidden_tags: vec![],
            notify_failure: true,
            notify_completion: false,
            failures_seen_at: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no config directory on this system")]
    NoConfigDir,
    #[error("could not read or write the config: {0}")]
    Io(#[from] std::io::Error),
    #[error("the config file is not valid: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("credential store: {0}")]
    Keyring(#[from] keyring::Error),
}

impl Config {
    pub fn path() -> Result<PathBuf, StoreError> {
        Ok(dirs::config_dir().ok_or(StoreError::NoConfigDir)?.join("dagpeek").join("config.json"))
    }

    pub fn load() -> Result<Self, StoreError> {
        match std::fs::read(Self::path()?) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self) -> Result<(), StoreError> {
        let path = Self::path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn active(&self) -> Option<&Server> {
        self.active_server
            .as_ref()
            .and_then(|id| self.servers.iter().find(|s| &s.id == id))
            .or_else(|| self.servers.first())
    }

    /// Whether a hide rule (by DAG name or tag) removes this run from view.
    pub fn is_hidden(&self, run: &DagRun) -> bool {
        self.hidden_names.iter().any(|n| n == &run.name)
            || run.all_tags().any(|t| self.hidden_tags.contains(t))
    }
}

fn entry(server_id: &str) -> Result<keyring::Entry, StoreError> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, server_id)?)
}

pub fn set_secret(server_id: &str, secret: &str) -> Result<(), StoreError> {
    Ok(entry(server_id)?.set_password(secret)?)
}

pub fn delete_secret(server_id: &str) -> Result<(), StoreError> {
    match entry(server_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl Server {
    pub fn auth(&self) -> Result<Auth, StoreError> {
        Ok(match self.auth {
            AuthKind::None => Auth::None,
            AuthKind::Basic => Auth::Basic {
                user: self.username.clone().unwrap_or_default(),
                password: entry(&self.id)?.get_password()?,
            },
            AuthKind::Bearer => Auth::Bearer(entry(&self.id)?.get_password()?),
        })
    }
}
