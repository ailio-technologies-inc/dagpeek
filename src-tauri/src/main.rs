// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg_attr(target_os = "macos", allow(dead_code))]
mod tray_icon;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dagpeek_core::client::{RunFilter, STATUS_SUCCESS};
use dagpeek_core::poller::poll_interval;
use dagpeek_core::progress::compute;
use dagpeek_core::store::{delete_secret, set_secret};
use dagpeek_core::*;
use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Rect, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Notify;
use tray_icon::Glyph;

const HISTORY_TTL: Duration = Duration::from_secs(30 * 60);
const POPOVER_GAP: f64 = 8.0;
const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunView {
    run: DagRun,
    progress: Option<Progress>,
    steps: Vec<Node>,
    url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueGroup {
    name: String,
    items: Vec<RunView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    has_server: bool,
    server_name: Option<String>,
    server_url: Option<String>,
    connection: Connection,
    version: Option<String>,
    failed: Vec<RunView>,
    running: Vec<RunView>,
    queued: Vec<QueueGroup>,
    workers: Vec<Worker>,
    taken_at: DateTime<Utc>,
}

impl View {
    fn empty(has_server: bool, connection: Connection) -> Self {
        Self {
            has_server,
            server_name: None,
            server_url: None,
            connection,
            version: None,
            failed: vec![],
            running: vec![],
            queued: vec![],
            workers: vec![],
            taken_at: Utc::now(),
        }
    }
}

#[derive(Default)]
struct Watch {
    history: HashMap<String, (Instant, History)>,
    seen_failed: Option<HashSet<String>>,
    seen_running: Option<HashSet<(String, String)>>,
    backoff: Backoff,
    server_id: Option<String>,
}

struct AppState {
    config: Mutex<Config>,
    view: Mutex<View>,
    popover_open: Mutex<bool>,
    hidden_at: Mutex<Option<Instant>>,
    refresh: Notify,
    tray: Mutex<Option<TrayIcon>>,
}

async fn history_for(client: &Client, watch: &mut Watch, name: &str) -> History {
    if let Some((at, h)) = watch.history.get(name) {
        if at.elapsed() < HISTORY_TTL {
            return h.clone();
        }
    }
    // The run list is bounded by date, so the window is explicit.
    let filter = RunFilter {
        name: Some(name),
        statuses: &[STATUS_SUCCESS],
        from: Some(Utc::now() - chrono::Duration::days(90)),
        limit: Some(10),
    };
    let mut details = vec![];
    for run in client.runs(&filter).await.unwrap_or_default() {
        if let Ok(d) = client.run_details(&run.name, &run.dag_run_id).await {
            details.push(d);
        }
    }
    let history = History::from_runs(&details);
    watch.history.insert(name.to_string(), (Instant::now(), history.clone()));
    history
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn run_key(r: &DagRun) -> String {
    format!("{}/{}", r.name, r.dag_run_id)
}

/// Refreshes the view and returns how long to wait before the next look.
async fn poll_once(app: &AppHandle, watch: &mut Watch) -> Duration {
    let state = app.state::<AppState>();
    let config = state.config.lock().unwrap().clone();
    let popover_open = *state.popover_open.lock().unwrap();

    let Some(server) = config.active().cloned() else {
        publish(app, View::empty(false, Connection::Ok));
        return poll_interval(popover_open, false);
    };
    if watch.server_id.as_deref() != Some(&server.id) {
        *watch = Watch { server_id: Some(server.id.clone()), ..Default::default() };
    }
    let auth = match server.auth() {
        Ok(a) => a,
        Err(_) => {
            publish(app, View::empty(true, Connection::AuthFailed));
            return poll_interval(popover_open, false);
        }
    };

    let now = Utc::now();
    let client = Client::new(&server.url, auth);
    let snap = fetch_snapshot(&client, now - chrono::Duration::days(1), now).await;

    if snap.connection != Connection::Ok {
        watch.backoff.record_failure();
        let mut view = View::empty(true, snap.connection);
        view.server_name = Some(server.name.clone());
        view.server_url = Some(server.url.clone());
        publish(app, view);
        return watch.backoff.delay().unwrap_or(Duration::from_secs(5));
    }
    watch.backoff.reset();

    let visible = |r: &DagRun| !config.is_hidden(r);
    let plain = |run: DagRun| RunView {
        url: client.run_url(&run.name, &run.dag_run_id),
        progress: None,
        steps: vec![],
        run,
    };

    let mut running = vec![];
    for run in snap.running.iter().filter(|r| visible(r)).cloned() {
        // Run detail is only fetched for active runs.
        match client.run_details(&run.name, &run.dag_run_id).await {
            Ok(details) => {
                let history = history_for(&client, watch, &run.name).await;
                running.push(RunView {
                    url: client.run_url(&run.name, &run.dag_run_id),
                    progress: Some(compute(&details, Some(&history), now)),
                    steps: details.nodes,
                    run,
                });
            }
            Err(_) => running.push(plain(run)),
        }
    }

    let mut queued = vec![];
    for queue in snap.queues.iter().filter(|q| q.queued_count > 0) {
        let items: Vec<RunView> = client
            .queued_items(&queue.name)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| visible(r))
            .map(&plain)
            .collect();
        if !items.is_empty() {
            queued.push(QueueGroup { name: queue.name.clone(), items });
        }
    }

    let failed: Vec<DagRun> = snap
        .failed
        .iter()
        .filter(|r| visible(r))
        .filter(|r| {
            let seen = config.failures_seen_at.into_iter().chain(config.failures_seen_by_dag.get(&r.name).copied()).max();
            match (seen, r.finished_at) {
                (Some(seen), Some(done)) => done > seen,
                _ => true,
            }
        })
        .cloned()
        .collect();

    // Nothing is announced on the first look, only changes after it.
    let failed_ids: HashSet<String> = snap.failed.iter().map(run_key).collect();
    if let Some(seen) = &watch.seen_failed {
        if config.notify_failure {
            for run in failed.iter().filter(|r| !seen.contains(&run_key(r))) {
                notify(app, "Run failed", &run.name);
            }
        }
    }
    watch.seen_failed = Some(failed_ids);

    let running_ids: HashSet<(String, String)> =
        snap.running.iter().filter(|r| visible(r)).map(|r| (r.name.clone(), r.dag_run_id.clone())).collect();
    if let Some(seen) = &watch.seen_running {
        if config.notify_completion {
            for (name, _) in seen.difference(&running_ids) {
                notify(app, "Run finished", name);
            }
        }
    }
    watch.seen_running = Some(running_ids);

    let queued_total: usize = queued.iter().map(|g: &QueueGroup| g.items.len()).sum();
    let active = !running.is_empty() || queued_total > 0;
    publish(
        app,
        View {
            has_server: true,
            server_name: Some(server.name),
            server_url: Some(server.url),
            connection: Connection::Ok,
            version: snap.version,
            failed: failed.into_iter().map(&plain).collect(),
            running,
            queued,
            workers: snap.workers,
            taken_at: now,
        },
    );
    poll_interval(popover_open, active)
}

fn publish(app: &AppHandle, view: View) {
    let state = app.state::<AppState>();
    update_tray(&state, &view);
    let _ = app.emit("view", &view);
    *state.view.lock().unwrap() = view;
}

fn update_tray(state: &AppState, view: &View) {
    let guard = state.tray.lock().unwrap();
    let Some(tray) = guard.as_ref() else { return };
    let running = view.running.len();
    let queued: usize = view.queued.iter().map(|g| g.items.len()).sum();
    // DAGs that are failing, not runs: one DAG failing every 15 minutes is one problem.
    let failures = view.failed.iter().map(|f| &f.run.name).collect::<HashSet<_>>().len();

    let glyph = match &view.connection {
        Connection::Ok if running == 1 => {
            Glyph::Active { fraction: view.running[0].progress.as_ref().and_then(|p| p.fraction) }
        }
        Connection::Ok if running > 1 => Glyph::Active { fraction: None },
        Connection::Ok => Glyph::Idle,
        _ => Glyph::Unknown,
    };

    let tooltip = match &view.connection {
        Connection::Ok if !view.has_server => "Dagpeek: add a server".to_string(),
        Connection::Ok => format!("Dagpeek: {running} running, {queued} queued, {failures} failed"),
        Connection::Unreachable(_) => "Dagpeek: server unreachable".to_string(),
        Connection::AuthFailed => "Dagpeek: auth failed".to_string(),
        Connection::Error(_) => "Dagpeek: server error".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));

    #[cfg(target_os = "macos")]
    {
        let drawn = tray_icon::draw_macos(glyph);
        let _ = tray.set_icon(Some(Image::new_owned(drawn.rgba, drawn.size, drawn.size)));
        let _ = tray.set_icon_as_template(true);
        let known = view.connection == Connection::Ok;
        let _ = tray.set_title(known.then(|| tray_icon::title(running, queued, failures)).flatten());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let dark = true; // Windows' default taskbar; the ink stays legible on the light one too.
        let drawn = tray_icon::draw_windows(glyph, running, failures, dark);
        let _ = tray.set_icon(Some(Image::new_owned(drawn.rgba, drawn.size, drawn.size)));
    }
}

async fn poll_loop(app: AppHandle) {
    let mut watch = Watch::default();
    let mut next = Instant::now();
    let mut last_tick = Utc::now();
    loop {
        let state = app.state::<AppState>();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            _ = state.refresh.notified() => next = Instant::now(),
        }
        // A one-second tick that took far longer means the machine slept: refresh at once.
        let wall = Utc::now();
        if (wall - last_tick).num_seconds() > 10 {
            next = Instant::now();
        }
        last_tick = wall;
        if Instant::now() >= next {
            let wait = poll_once(&app, &mut watch).await;
            next = Instant::now() + wait;
        }
    }
}

/// Puts the popover against the tray icon, on whichever taskbar edge that is.
fn place_popover(window: &WebviewWindow, rect: Rect) {
    let Ok(Some(monitor)) = window.primary_monitor().map(|fallback| {
        let scale = fallback.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
        let at: PhysicalPosition<f64> = rect.position.to_physical(scale);
        window.monitor_from_point(at.x, at.y).ok().flatten().or(fallback)
    }) else {
        return;
    };
    let scale = monitor.scale_factor();
    let at: PhysicalPosition<f64> = rect.position.to_physical(scale);
    let icon: tauri::PhysicalSize<f64> = rect.size.to_physical(scale);
    let size = window.outer_size().unwrap_or_default();
    let (w, h) = (size.width as f64, size.height as f64);
    let gap = POPOVER_GAP * scale;

    let area = monitor.work_area();
    let (ax, ay) = (area.position.x as f64, area.position.y as f64);
    let (aw, ah) = (area.size.width as f64, area.size.height as f64);

    let mid_x = at.x + icon.width / 2.0;
    let mid_y = at.y + icon.height / 2.0;
    let on_side = at.x + icon.width <= ax || at.x >= ax + aw;

    let (x, y) = if on_side {
        // Taskbar at the left or right: open beside it, level with the icon.
        let x = if mid_x < ax + aw / 2.0 { ax + gap } else { ax + aw - w - gap };
        (x, mid_y - h / 2.0)
    } else {
        // Menu bar or a top or bottom taskbar: open below or above the icon.
        let y = if mid_y < ay + ah / 2.0 { at.y + icon.height + gap } else { at.y - h - gap };
        (mid_x - w / 2.0, y)
    };

    let x = x.clamp(ax + gap, (ax + aw - w - gap).max(ax + gap));
    let y = y.clamp(ay + gap, (ay + ah - h - gap).max(ay + gap));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn hide_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popover") else { return };
    let _ = window.hide();
    let state = app.state::<AppState>();
    *state.popover_open.lock().unwrap() = false;
    *state.hidden_at.lock().unwrap() = Some(Instant::now());
}

fn toggle_popover(app: &AppHandle, rect: Rect) {
    let Some(window) = app.get_webview_window("popover") else { return };
    let state = app.state::<AppState>();
    // Clicking the icon while open blurs the popover first; that click must not reopen it.
    let just_hidden = state.hidden_at.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(250));
    if window.is_visible().unwrap_or(false) || just_hidden {
        hide_popover(app);
        return;
    }
    place_popover(&window, rect);
    let _ = window.show();
    let _ = window.set_focus();
    *state.popover_open.lock().unwrap() = true;
    state.refresh.notify_one();
}

#[tauri::command]
fn get_view(state: State<AppState>) -> View {
    state.view.lock().unwrap().clone()
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

fn store(state: &AppState, change: impl FnOnce(&mut Config)) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    change(&mut config);
    config.save().map_err(|e| e.to_string())?;
    state.refresh.notify_one();
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Prefs {
    hidden_names: Vec<String>,
    hidden_tags: Vec<String>,
    notify_failure: bool,
    notify_completion: bool,
}

#[tauri::command]
fn save_prefs(state: State<AppState>, prefs: Prefs) -> Result<(), String> {
    store(&state, |c| {
        c.hidden_names = prefs.hidden_names;
        c.hidden_tags = prefs.hidden_tags;
        c.notify_failure = prefs.notify_failure;
        c.notify_completion = prefs.notify_completion;
    })
}

/// Adds or updates a server. `secret`, when given, goes to the OS credential store and nowhere else.
#[tauri::command]
fn save_server(state: State<AppState>, server: Server, secret: Option<String>) -> Result<(), String> {
    match (server.auth, secret.filter(|s| !s.is_empty())) {
        (AuthKind::None, _) => delete_secret(&server.id).map_err(|e| e.to_string())?,
        (_, Some(s)) => set_secret(&server.id, &s).map_err(|e| e.to_string())?,
        (_, None) => {}
    }
    store(&state, |c| {
        if c.active_server.is_none() {
            c.active_server = Some(server.id.clone());
        }
        match c.servers.iter_mut().find(|s| s.id == server.id) {
            Some(existing) => *existing = server,
            None => c.servers.push(server),
        }
    })
}

#[tauri::command]
fn delete_server(state: State<AppState>, id: String) -> Result<(), String> {
    delete_secret(&id).map_err(|e| e.to_string())?;
    store(&state, |c| {
        c.servers.retain(|s| s.id != id);
        if c.active_server.as_deref() == Some(&id) {
            c.active_server = c.servers.first().map(|s| s.id.clone());
        }
    })
}

#[tauri::command]
fn set_active_server(state: State<AppState>, id: String) -> Result<(), String> {
    store(&state, |c| c.active_server = Some(id))
}

#[tauri::command]
fn dismiss_failures(state: State<AppState>) -> Result<(), String> {
    store(&state, |c| c.failures_seen_at = Some(Utc::now()))
}

#[tauri::command]
fn dismiss_failure(state: State<AppState>, name: String) -> Result<(), String> {
    store(&state, |c| {
        c.failures_seen_by_dag.insert(name, Utc::now());
    })
}

/// Tries a server before it is saved. Reports the same states the tray would show.
#[tauri::command]
async fn test_server(url: String, kind: AuthKind, username: Option<String>, secret: Option<String>) -> Result<String, String> {
    let secret = secret.unwrap_or_default();
    let auth = match kind {
        AuthKind::None => Auth::None,
        AuthKind::Basic => Auth::Basic { user: username.unwrap_or_default(), password: secret },
        AuthKind::Bearer => Auth::Bearer(secret),
    };
    let client = Client::new(&url, auth);
    let health = client.health().await.map_err(|e| e.to_string())?;
    client.runs(&RunFilter { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
    Ok(health.version.unwrap_or_else(|| "unknown".into()))
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only web links open from here".into());
    }
    hide_popover(&app);
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Installs a newer version if there is one, and returns its number.
async fn install_update(app: &AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let version = update.version.clone();
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    Ok(Some(version))
}

/// Restarts into an installed update. A menu bar app is rarely opened again, so waiting for
/// its next launch could mean months. It waits for the popover to close, so nothing vanishes
/// from under someone reading it.
async fn restart_when_idle(app: AppHandle, version: String) {
    notify(&app, "Dagpeek updated", &format!("Now on version {version}."));
    while *app.state::<AppState>().popover_open.lock().unwrap() {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    app.restart();
}

async fn update_loop(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        if let Ok(Some(version)) = install_update(&app).await {
            restart_when_idle(app.clone(), version).await;
        }
        tokio::time::sleep(UPDATE_INTERVAL).await;
    }
}

#[tauri::command]
async fn check_updates(app: AppHandle) -> Result<String, String> {
    match install_update(&app).await? {
        Some(version) => {
            tauri::async_runtime::spawn(restart_when_idle(app, version.clone()));
            Ok(format!("Version {version} is installed. Dagpeek restarts when you close this."))
        }
        None => Ok("Dagpeek is up to date.".to_string()),
    }
}

fn main() {
    let config = Config::load().unwrap_or_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| {}))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config: Mutex::new(config),
            view: Mutex::new(View::empty(false, Connection::Ok)),
            popover_open: Mutex::new(false),
            hidden_at: Mutex::new(None),
            refresh: Notify::new(),
            tray: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_view, get_config, save_prefs, save_server, delete_server, set_active_server,
            dismiss_failures, dismiss_failure, test_server, open_url, check_updates, quit
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                hide_popover(window.app_handle());
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let quit_item = MenuItem::with_id(app, "quit", "Quit Dagpeek", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;
            let first = tray_icon::draw_macos(Glyph::Unknown);
            let tray = TrayIconBuilder::with_id("main")
                .icon(Image::new_owned(first.rgba, first.size, first.size))
                .icon_as_template(cfg!(target_os = "macos"))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle_popover(tray.app_handle(), rect);
                    }
                })
                .build(app)?;
            *app.state::<AppState>().tray.lock().unwrap() = Some(tray);

            tauri::async_runtime::spawn(poll_loop(app.handle().clone()));
            tauri::async_runtime::spawn(update_loop(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Dagpeek could not start");
}
