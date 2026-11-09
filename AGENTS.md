# Dagpeek

A free tray app for Dagu, by Ailio. Tauri 2: `crates/core` (API client, progress, store), `crates/cli`, `src-tauri`, `ui`, `site`.

- All Dagu calls are in Rust, never in the webview. Secrets go to the OS credential store only.
- Actions on a Dagu server (stop, retry, dequeue) are on by default, can be turned off in Settings (checked again in Rust), and are always confirmed.
- Unknown is not zero. Unreachable or refused never renders as "0 running". No ETA with fewer than 3 past successes.
- The app's only telemetry is the daily count in crates/core/usage.rs: a random id, version and OS, never servers, DAGs or runs, and Settings turns it off. dagpeek.com loads Google Analytics only after consent.
- Test: `cargo test --workspace`. Site deploys with `wrangler deploy` from `site/`.

## Fixes are never one-offs

A fix that exists only on a running machine is not done. Every change to infrastructure, a host, a container, a setting or a provider's console lands in the code that owns it, in the same session, so the next run of that code keeps it instead of quietly undoing it.

- **Terraform first.** If Terraform manages the thing, or runs the script that sets it up (infra's `infra/terraform/cluster.tf` jobs run the tower and worker setup scripts), change that code and apply it the normal way. If it isn't managed yet and could be, bring it under Terraform as part of the fix.
- **A live change is half a fix.** When an outage forces a live change first (`docker update`, a console toggle, an edited file on a host), make the same change in code before the session ends, commit it, and say that both happened. A live change with no commit is a bug waiting for the next apply.
- **Never edit a dashboard, host or console alone.** If it can't be expressed in code yet, write down exactly what was changed and why, and treat bringing it under code as open work.
- **Infrastructure lives in [ailio-technologies-inc/infra](https://github.com/ailio-technologies-inc/infra).** A fix to the cluster, a host, DNS, Cloudflare or another provider goes there, even when the symptom showed up in this repo.
