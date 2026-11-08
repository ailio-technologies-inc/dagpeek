# Dagpeek

A free tray app for Dagu, by Ailio. Tauri 2: `crates/core` (API client, progress, store), `crates/cli`, `src-tauri`, `ui`, `site`.

- All Dagu calls are in Rust, never in the webview. Secrets go to the OS credential store only.
- Read-only: no action that changes anything on a Dagu server.
- Unknown is not zero. Unreachable or refused never renders as "0 running". No ETA with fewer than 3 past successes.
- No analytics or telemetry, on the app or on dagpeek.com.
- Test: `cargo test --workspace`. Site deploys with `wrangler deploy` from `site/`.
