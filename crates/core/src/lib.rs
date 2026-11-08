//! Everything Dagpeek knows about a Dagu server, shared by the tray app and the CLI.

pub mod client;
pub mod models;
pub mod poller;
pub mod progress;
pub mod store;
pub mod time;

pub use client::{Auth, Client, Error};
pub use models::*;
pub use poller::{fetch_snapshot, Backoff, Connection, Snapshot};
pub use progress::{History, Progress};
pub use store::{AuthKind, Config, MenuBarStyle, Server};
