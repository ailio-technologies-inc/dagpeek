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
const ALL_SERVERS: &str = "*";
const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const UPDATE_RETRY: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunView {
    run: DagRun,
    progress: Option<Progress>,
    steps: Vec<Node>,
    url: String,
    server_id: String,
    server_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueGroup {
    name: String,
    items: Vec<RunView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerChoice {
    id: String,
    name: String,
    active: bool,
}

/// A server that can't be read while others can.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Problem {
    server_name: String,
    connection: Connection,
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
    all_servers: bool,
    servers: Vec<ServerChoice>,
    problems: Vec<Problem>,
    allow_actions: bool,
    terms_accepted: bool,
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
            all_servers: false,
            servers: vec![],
            problems: vec![],
            allow_actions: false,
            terms_accepted: false,
        }
    }
}

/// What one server last reported.
#[derive(Debug, Clone)]
struct ServerView {
    connection: Connection,
    version: Option<String>,
    failed: Vec<RunView>,
    running: Vec<RunView>,
    queued: Vec<QueueGroup>,
    workers: Vec<Worker>,
}

impl ServerView {
    fn down(connection: Connection) -> Self {
        Self { connection, version: None, failed: vec![], running: vec![], queued: vec![], workers: vec![] }
    }
}

#[derive(Default)]
struct Watch {
    history: HashMap<String, (Instant, History)>,
    seen_failed: Option<HashSet<String>>,
    seen_running: Option<HashSet<(String, String)>>,
    backoff: Backoff,
    next: Option<Instant>,
    last: Option<ServerView>,
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

/// One look at one server, and how long to wait before the next.
async fn poll_server(app: &AppHandle, config: &Config, server: &Server, multi: bool, popover_open: bool, watch: &mut Watch) -> (ServerView, Duration) {
    let auth = match server.auth() {
        Ok(a) => a,
        Err(_) => return (ServerView::down(Connection::AuthFailed), poll_interval(popover_open, false)),
    };

    let now = Utc::now();
    let client = Client::new(&server.url, auth);
    let snap = fetch_snapshot(&client, now - chrono::Duration::days(1), now).await;

    if snap.connection != Connection::Ok {
        watch.backoff.record_failure();
        return (ServerView::down(snap.connection), watch.backoff.delay().unwrap_or(Duration::from_secs(5)));
    }
    watch.backoff.reset();

    let visible = |r: &DagRun| !config.is_hidden(r);
    let plain = |run: DagRun| RunView {
        url: client.run_url(&run.name, &run.dag_run_id),
        progress: None,
        steps: vec![],
        run,
        server_id: server.id.clone(),
        server_name: server.name.clone(),
    };

    let mut running = vec![];
    for run in snap.running.iter().filter(|r| visible(r)).cloned() {
        // Run detail is only fetched for active runs.
        match client.run_details(&run.name, &run.dag_run_id).await {
            Ok(details) => {
                let history = history_for(&client, watch, &run.name).await;
                let progress = Some(compute(&details, Some(&history), now));
                running.push(RunView { progress, steps: details.nodes, ..plain(run) });
            }
            Err(_) => running.push(plain(run)),
        }
    }

    let mut queued = vec![];
    for queue in snap.queues.iter().filter(|q| q.queued_count > 0) {
        let items: Vec<RunView> =
            client.queued_items(&queue.name).await.unwrap_or_default().into_iter().filter(|r| visible(r)).map(&plain).collect();
        if !items.is_empty() {
            let name = if multi { format!("{} on {}", queue.name, server.name) } else { queue.name.clone() };
            queued.push(QueueGroup { name, items });
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

    let where_ = |name: &str| if multi { format!("{name} on {}", server.name) } else { name.to_string() };

    // Nothing is announced on the first look, only changes after it.
    let failed_ids: HashSet<String> = snap.failed.iter().map(run_key).collect();
    if let Some(seen) = &watch.seen_failed {
        if config.notify_failure {
            for run in failed.iter().filter(|r| !seen.contains(&run_key(r))) {
                notify(app, "Run failed", &where_(&run.name));
            }
        }
    }
    watch.seen_failed = Some(failed_ids);

    let running_ids: HashSet<(String, String)> =
        snap.running.iter().filter(|r| visible(r)).map(|r| (r.name.clone(), r.dag_run_id.clone())).collect();
    if let Some(seen) = &watch.seen_running {
        if config.notify_completion {
            for (name, _) in seen.difference(&running_ids) {
                notify(app, "Run finished", &where_(name));
            }
        }
    }
    watch.seen_running = Some(running_ids);

    let active = !running.is_empty() || !queued.is_empty();
    let view = ServerView {
        connection: Connection::Ok,
        version: snap.version,
        failed: failed.into_iter().map(&plain).collect(),
        running,
        queued,
        workers: snap.workers,
    };
    (view, poll_interval(popover_open, active))
}

fn merge(config: &Config, targets: &[Server], watches: &HashMap<String, Watch>) -> View {
    let multi = targets.len() > 1;
    let seen: Vec<(&Server, &ServerView)> =
        targets.iter().filter_map(|s| watches.get(&s.id).and_then(|w| w.last.as_ref()).map(|v| (s, v))).collect();

    let mut view = View::empty(true, Connection::Ok);
    view.all_servers = multi;
    view.allow_actions = config.allow_actions;
    view.servers = config
        .servers
        .iter()
        .map(|s| ServerChoice {
            id: s.id.clone(),
            name: s.name.clone(),
            active: !config.show_all_servers && config.active().is_some_and(|a| a.id == s.id),
        })
        .collect();
    if multi {
        view.server_name = Some("All servers".to_string());
    } else if let Some(server) = targets.first() {
        view.server_name = Some(server.name.clone());
        view.server_url = Some(server.url.clone());
    }

    let reachable = seen.iter().any(|(_, v)| v.connection == Connection::Ok);
    if !reachable {
        if let Some((_, v)) = seen.first() {
            view.connection = v.connection.clone();
        }
    }
    for (server, v) in &seen {
        if v.connection != Connection::Ok && reachable {
            view.problems.push(Problem { server_name: server.name.clone(), connection: v.connection.clone() });
        }
        view.failed.extend(v.failed.iter().cloned());
        view.running.extend(v.running.iter().cloned());
        view.queued.extend(v.queued.iter().cloned());
        view.workers.extend(v.workers.iter().cloned());
        if !multi {
            view.version = v.version.clone();
        }
    }
    view.failed.sort_by(|a, b| b.run.finished_at.cmp(&a.run.finished_at));
    view.running.sort_by(|a, b| b.run.started_at.cmp(&a.run.started_at));
    view
}

/// Looks at every server that is due, publishes the combined view, and returns how long to wait.
async fn poll_once(app: &AppHandle, watches: &mut HashMap<String, Watch>) -> Duration {
    let state = app.state::<AppState>();
    let config = state.config.lock().unwrap().clone();
    let popover_open = *state.popover_open.lock().unwrap();

    let targets: Vec<Server> = if config.show_all_servers { config.servers.clone() } else { config.active().cloned().into_iter().collect() };
    if targets.is_empty() {
        publish(app, View::empty(false, Connection::Ok));
        return poll_interval(popover_open, false);
    }
    watches.retain(|id, _| targets.iter().any(|s| &s.id == id));

    let multi = targets.len() > 1;
    let mut wait = Duration::from_secs(300);
    for server in &targets {
        let watch = watches.entry(server.id.clone()).or_default();
        if watch.next.is_none_or(|next| Instant::now() >= next) {
            let (view, delay) = poll_server(app, &config, server, multi, popover_open, watch).await;
            watch.last = Some(view);
            watch.next = Some(Instant::now() + delay);
        }
        if let Some(next) = watch.next {
            wait = wait.min(next.saturating_duration_since(Instant::now()));
        }
    }
    publish(app, merge(&config, &targets, watches));
    wait.max(Duration::from_secs(1))
}

fn publish(app: &AppHandle, mut view: View) {
    let state = app.state::<AppState>();
    view.terms_accepted = state.config.lock().unwrap().terms_accepted_at.is_some();
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
    let failures = view.failed.iter().map(|f| (&f.server_id, &f.run.name)).collect::<HashSet<_>>().len();

    let fractions: Vec<Option<f64>> = view.running.iter().map(|r| r.progress.as_ref().and_then(|p| p.fraction)).collect();
    let glyph = match &view.connection {
        // A server that can't be read might be busy: unknown is never drawn as idle.
        Connection::Ok if running == 0 && !view.problems.is_empty() => Glyph::Unknown,
        Connection::Ok if running > 0 => {
            // One ring: the combined progress of everything running.
            let known: Vec<f64> = fractions.iter().copied().flatten().collect();
            Glyph::Active { fraction: (!known.is_empty()).then(|| known.iter().sum::<f64>() / known.len() as f64) }
        }
        Connection::Ok => Glyph::Idle,
        _ => Glyph::Unknown,
    };

    let tooltip = match &view.connection {
        Connection::Ok if !view.has_server => "Dagpeek: add a server".to_string(),
        Connection::Ok if !view.problems.is_empty() => {
            format!("Dagpeek: {running} running, {queued} queued, {failures} failed, {} unreachable", view.problems.len())
        }
        Connection::Ok => format!("Dagpeek: {running} running, {queued} queued, {failures} failed"),
        Connection::Unreachable(_) => "Dagpeek: server unreachable".to_string(),
        Connection::AuthFailed => "Dagpeek: auth failed".to_string(),
        Connection::Error(_) => "Dagpeek: server error".to_string(),
    };
    let _ = tray.set_tooltip(Some(tooltip));

    #[cfg(target_os = "macos")]
    {
        let drawn = tray_icon::draw_macos(glyph, failures > 0);
        let _ = tray.set_icon(Some(Image::new_owned(drawn.rgba, drawn.width, drawn.size)));
        let _ = tray.set_icon_as_template(true);
        let known = view.connection == Connection::Ok;
        let style = state.config.lock().unwrap().menu_bar_style;
        let single = (running == 1).then(|| view.running[0].progress.as_ref()).flatten();
        let _ = tray.set_title(known.then(|| tray_icon::title(style, running, queued, failures, single)).flatten());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let dark = true; // Windows' default taskbar; the ink stays legible on the light one too.
        let drawn = tray_icon::draw_windows(glyph, running, failures, dark);
        let _ = tray.set_icon(Some(Image::new_owned(drawn.rgba, drawn.width, drawn.size)));
    }
}

async fn poll_loop(app: AppHandle) {
    let mut watches: HashMap<String, Watch> = HashMap::new();
    let mut next = Instant::now();
    let mut last_tick = Utc::now();
    loop {
        let state = app.state::<AppState>();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            _ = state.refresh.notified() => {
                next = Instant::now();
                watches.values_mut().for_each(|w| w.next = None);
            }
        }
        // A one-second tick that took far longer means the machine slept: refresh at once.
        let wall = Utc::now();
        if (wall - last_tick).num_seconds() > 10 {
            next = Instant::now();
            watches.values_mut().for_each(|w| w.next = None);
        }
        last_tick = wall;
        if Instant::now() >= next {
            let wait = poll_once(&app, &mut watches).await;
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

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
}

/// Closes the popover on a click anywhere else. A menu bar app often never becomes the active
/// app, so losing focus alone can't be relied on to report that.
#[cfg(target_os = "macos")]
async fn close_on_outside_click(app: AppHandle) {
    const COMBINED_SESSION: i32 = 0;
    let Some(window) = app.get_webview_window("popover") else { return };
    while *app.state::<AppState>().popover_open.lock().unwrap() {
        // SAFETY: a read-only query taking plain integers.
        let pressed = unsafe { CGEventSourceButtonState(COMBINED_SESSION, 0) || CGEventSourceButtonState(COMBINED_SESSION, 1) };
        if pressed {
            let inside = match (app.cursor_position(), window.outer_position(), window.outer_size()) {
                (Ok(c), Ok(p), Ok(s)) => {
                    c.x >= p.x as f64 && c.x < p.x as f64 + s.width as f64 && c.y >= p.y as f64 && c.y < p.y as f64 + s.height as f64
                }
                _ => true,
            };
            if !inside {
                hide_popover(&app);
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn hide_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popover") else { return };
    let _ = window.hide();
    let state = app.state::<AppState>();
    *state.popover_open.lock().unwrap() = false;
    *state.hidden_at.lock().unwrap() = Some(Instant::now());
}

fn show_popover(app: &AppHandle, window: &WebviewWindow, rect: Rect) {
    place_popover(window, rect);
    let _ = window.show();
    let _ = window.set_focus();
    let state = app.state::<AppState>();
    *state.popover_open.lock().unwrap() = true;
    state.refresh.notify_one();
    let _ = app.emit("popover-opened", ());
    #[cfg(target_os = "macos")]
    tauri::async_runtime::spawn(close_on_outside_click(app.clone()));
}

/// Opens the popover by itself on first run, so the terms are seen.
async fn open_for_terms(app: AppHandle) {
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let state = app.state::<AppState>();
    if state.config.lock().unwrap().terms_accepted_at.is_some() {
        return;
    }
    let rect = state.tray.lock().unwrap().as_ref().and_then(|t| t.rect().ok().flatten());
    if let (Some(rect), Some(window)) = (rect, app.get_webview_window("popover")) {
        show_popover(&app, &window, rect);
    }
}

fn toggle_popover(app: &AppHandle, rect: Rect) {
    let Some(window) = app.get_webview_window("popover") else { return };
    let state = app.state::<AppState>();
    // Clicking the icon while open blurs the popover first; that click must not reopen it.
    let just_hidden = state.hidden_at.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(500));
    if window.is_visible().unwrap_or(false) || just_hidden {
        hide_popover(app);
        return;
    }
    show_popover(app, &window, rect);
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
    menu_bar_style: MenuBarStyle,
    allow_actions: bool,
    usage_stats: bool,
}

#[tauri::command]
fn save_prefs(state: State<AppState>, prefs: Prefs) -> Result<(), String> {
    store(&state, |c| {
        c.hidden_names = prefs.hidden_names;
        c.hidden_tags = prefs.hidden_tags;
        c.notify_failure = prefs.notify_failure;
        c.notify_completion = prefs.notify_completion;
        c.menu_bar_style = prefs.menu_bar_style;
        c.allow_actions = prefs.allow_actions;
        c.usage_stats = prefs.usage_stats;
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
    store(&state, |c| {
        c.show_all_servers = id == ALL_SERVERS;
        if id != ALL_SERVERS {
            c.active_server = Some(id);
        }
    })
}

/// Stops, retries or dequeues a run. Refuses unless actions are turned on in Settings.
#[tauri::command]
async fn run_action(state: State<'_, AppState>, server_id: String, action: String, name: String, run_id: String) -> Result<(), String> {
    let (server, allowed) = {
        let config = state.config.lock().unwrap();
        (config.servers.iter().find(|s| s.id == server_id).cloned(), config.allow_actions)
    };
    if !allowed {
        return Err("Actions are off. Turn them on in Settings.".to_string());
    }
    let server = server.ok_or("That server is no longer set up.")?;
    let client = Client::new(&server.url, server.auth().map_err(|e| e.to_string())?);
    match action.as_str() {
        "stop" => client.stop_run(&name, &run_id).await,
        "retry" => client.retry_run(&name, &run_id).await,
        "dequeue" => client.dequeue_run(&name, &run_id).await,
        other => return Err(format!("Unknown action: {other}")),
    }
    .map_err(|e| e.to_string())?;
    state.refresh.notify_one();
    Ok(())
}

#[tauri::command]
fn dismiss_failures(state: State<AppState>) -> Result<(), String> {
    store(&state, |c| c.failures_seen_at = Some(Utc::now()))
}

#[tauri::command]
fn accept_terms(state: State<AppState>, usage_stats: bool) -> Result<(), String> {
    store(&state, |c| {
        c.terms_accepted_at = Some(Utc::now());
        c.usage_stats = usage_stats;
    })
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

/// At most one anonymous "active today" count a day, and none if it's turned off in Settings.
async fn usage_loop(app: AppHandle) {
    let Some(secret) = option_env!("DAGPEEK_GA_API_SECRET") else { return };
    loop {
        let state = app.state::<AppState>();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let due = {
            let mut config = state.config.lock().unwrap();
            if config.terms_accepted_at.is_some() && config.usage_stats && config.usage_sent_on.as_deref() != Some(today.as_str()) {
                let id = config.install_id.get_or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
                Some(id)
            } else {
                None
            }
        };
        if let Some(id) = due {
            let version = app.package_info().version.to_string();
            let os = std::env::consts::OS;
            if dagpeek_core::usage::send_active(secret, &id, &version, os, &today).await.is_ok() {
                let mut config = state.config.lock().unwrap();
                config.usage_sent_on = Some(today);
                let _ = config.save();
            }
        }
        tokio::time::sleep(Duration::from_secs(5 * 60)).await;
    }
}

async fn update_loop(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        // A failed check (network not up yet, say) tries again soon, not in six hours.
        let wait = match install_update(&app).await {
            Ok(Some(version)) => {
                restart_when_idle(app.clone(), version).await;
                UPDATE_INTERVAL
            }
            Ok(None) => UPDATE_INTERVAL,
            Err(_) => UPDATE_RETRY,
        };
        tokio::time::sleep(wait).await;
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
    // Every feature is on by default. Installs from before that get it once, and after that
    // whatever someone turns off stays off.
    let mut config = Config::load().unwrap_or_default();
    let first_defaults = !config.features_defaulted;
    if first_defaults {
        config.allow_actions = true;
        config.notify_failure = true;
        config.notify_completion = true;
        config.features_defaulted = true;
        let _ = config.save();
    }
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
            dismiss_failures, dismiss_failure, accept_terms, run_action, test_server, open_url, check_updates, quit
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                hide_popover(window.app_handle());
            }
        })
        .setup(move |app| {
            if first_defaults {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
            }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let quit_item = MenuItem::with_id(app, "quit", "Quit Dagpeek", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;
            let first = tray_icon::draw_macos(Glyph::Unknown, false);
            let tray = TrayIconBuilder::with_id("main")
                .icon(Image::new_owned(first.rgba, first.width, first.size))
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
            tauri::async_runtime::spawn(usage_loop(app.handle().clone()));
            tauri::async_runtime::spawn(open_for_terms(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Dagpeek could not start");
}
