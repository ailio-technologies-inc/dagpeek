//! `dagpeek status [--json]`, `dagpeek runs`, `dagpeek watch <dag>`, reading the app's server list.
//! `DAGPEEK_SERVER` points it at a URL instead.

use std::process::ExitCode;
use std::time::Duration;

use chrono::Utc;
use dagpeek_core::client::{RunFilter, STATUS_SUCCESS};
use dagpeek_core::progress::compute;
use dagpeek_core::*;

const USAGE: &str = "usage: dagpeek status [--json] | dagpeek runs | dagpeek watch <dag>";

fn client() -> Result<Client, String> {
    if let Ok(url) = std::env::var("DAGPEEK_SERVER") {
        return Ok(Client::new(&url, Auth::None));
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    let server = config
        .active()
        .ok_or("no server set up: add one in the Dagpeek app, or set DAGPEEK_SERVER")?;
    Ok(Client::new(&server.url, server.auth().map_err(|e| e.to_string())?))
}

async fn history(c: &Client, name: &str) -> History {
    let from = Utc::now() - chrono::Duration::days(90);
    let filter = RunFilter { name: Some(name), statuses: &[STATUS_SUCCESS], from: Some(from), limit: Some(10) };
    let mut details = vec![];
    for r in c.runs(&filter).await.unwrap_or_default() {
        if let Ok(d) = c.run_details(&r.name, &r.dag_run_id).await {
            details.push(d);
        }
    }
    History::from_runs(&details)
}

fn describe(p: &Progress) -> String {
    let step = p.current_step.as_deref().unwrap_or("-");
    let eta = p.eta_secs.map(|s| format!(", about {s}s left")).unwrap_or_default();
    format!("{} of {} steps, on {step}{eta}", p.done, p.total)
}

async fn status(c: &Client, json: bool) -> Result<(), String> {
    let now = Utc::now();
    let snap = fetch_snapshot(c, now - chrono::Duration::days(1), now).await;
    match &snap.connection {
        Connection::Ok => {}
        Connection::Unreachable(m) if !json => return Err(format!("unreachable: {m}")),
        Connection::AuthFailed if !json => return Err("auth failed: check this server's credentials".into()),
        Connection::Error(m) if !json => return Err(m.clone()),
        _ => {}
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?);
        return Ok(());
    }
    // The failure list is one page long, so a full page is "at least".
    let failed = if snap.failed.len() >= 50 { "50+".to_string() } else { snap.failed.len().to_string() };
    println!("{} running, {} queued, {failed} failed in the last day", snap.running.len(), snap.queued.len());
    for r in &snap.running {
        match c.run_details(&r.name, &r.dag_run_id).await {
            Ok(d) => println!("  {}: {}", r.name, describe(&compute(&d, Some(&history(c, &r.name).await), now))),
            Err(_) => println!("  {}", r.name),
        }
    }
    Ok(())
}

async fn runs(c: &Client) -> Result<(), String> {
    let list = c.runs(&RunFilter { limit: Some(25), ..Default::default() }).await.map_err(|e| e.to_string())?;
    for r in list {
        let when = r.started_at.map(|t| t.to_rfc3339()).unwrap_or_else(|| "-".into());
        println!("{:<22} {:<40} {}", String::from(r.status), r.name, when);
    }
    Ok(())
}

async fn watch(c: &Client, dag: &str) -> Result<(), String> {
    let hist = history(c, dag).await;
    let filter = RunFilter { name: Some(dag), limit: Some(1), ..Default::default() };
    loop {
        let run = c.runs(&filter).await.map_err(|e| e.to_string())?.into_iter().next()
            .ok_or_else(|| format!("no runs found for {dag}"))?;
        let details = c.run_details(&run.name, &run.dag_run_id).await.map_err(|e| e.to_string())?;
        println!("{}", describe(&compute(&details, Some(&hist), Utc::now())));
        if !matches!(details.status, RunStatus::Running | RunStatus::Queued | RunStatus::Waiting) {
            println!("{}", String::from(details.status));
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = async {
        let c = client()?;
        match args.as_slice() {
            ["status"] => status(&c, false).await,
            ["status", "--json"] => status(&c, true).await,
            ["runs"] => runs(&c).await,
            ["watch", dag] => watch(&c, dag).await,
            _ => Err(USAGE.to_string()),
        }
    }
    .await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dagpeek: {e}");
            ExitCode::FAILURE
        }
    }
}
