# Dagpeek

A free tray app for Dagu, by Ailio. Tauri 2: `crates/core` (API client, progress, store), `crates/cli`, `src-tauri`, `ui`, `site`.

- All Dagu calls are in Rust, never in the webview. Secrets go to the OS credential store only.
- Actions on a Dagu server (stop, retry, dequeue) are off by default, checked again in Rust, and always confirmed.
- Unknown is not zero. Unreachable or refused never renders as "0 running". No ETA with fewer than 3 past successes.
- The app's only telemetry is the daily count in crates/core/usage.rs: a random id, version and OS, never servers, DAGs or runs, and Settings turns it off. dagpeek.com loads Google Analytics only after consent.
- Test: `cargo test --workspace`. Site deploys with `wrangler deploy` from `site/`.
