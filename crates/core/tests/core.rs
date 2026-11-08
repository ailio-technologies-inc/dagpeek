use chrono::{DateTime, Duration, TimeZone, Utc};
use dagpeek_core::poller::poll_interval;
use dagpeek_core::progress::compute;
use dagpeek_core::time::parse;
use dagpeek_core::*;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

fn node(name: &str, status: &str, start: Option<i64>, end: Option<i64>) -> Node {
    Node {
        step: Step { name: name.into() },
        status: status.to_string().into(),
        started_at: start.map(|s| t0() + Duration::seconds(s)),
        finished_at: end.map(|s| t0() + Duration::seconds(s)),
        retry_count: 0,
    }
}

fn run(status: &str, nodes: Vec<Node>) -> RunDetails {
    RunDetails {
        name: "deploy".into(),
        dag_run_id: "r".into(),
        status: status.to_string().into(),
        started_at: Some(t0()),
        finished_at: None,
        nodes,
    }
}

/// `n` successful runs in which step a took 10 s and step b took 30 s.
fn past(n: usize) -> History {
    let runs: Vec<_> = (0..n)
        .map(|_| {
            run(
                "succeeded",
                vec![node("a", "succeeded", Some(0), Some(10)), node("b", "succeeded", Some(10), Some(40))],
            )
        })
        .collect();
    History::from_runs(&runs)
}

#[test]
fn timestamp_shapes() {
    assert_eq!(parse("2026-11-08T20:30:00Z"), Some(Utc.with_ymd_and_hms(2026, 11, 8, 20, 30, 0).unwrap()));
    let offset = parse("2026-11-08T14:11:51.699-07:00").unwrap();
    assert_eq!(offset.timestamp_millis(), Utc.with_ymd_and_hms(2026, 11, 8, 21, 11, 51).unwrap().timestamp_millis() + 699);
    assert_eq!(parse(""), None);
    assert_eq!(parse("-"), None);
    assert_eq!(parse("not a date"), None);
}

#[test]
fn unknown_status_labels_decode() {
    let r: DagRun = serde_json::from_str(
        r#"{"name":"x","dagRunId":"1","statusLabel":"brand_new","startedAt":"-","finishedAt":""}"#,
    )
    .unwrap();
    assert_eq!(r.status, RunStatus::Unknown("brand_new".into()));
    assert_eq!(r.started_at, None);
    assert_eq!(r.finished_at, None);
}

#[test]
fn counts_finished_steps() {
    let r = run(
        "running",
        vec![
            node("a", "succeeded", Some(0), Some(1)),
            node("b", "failed", Some(1), Some(2)),
            node("c", "skipped", None, None),
            node("d", "aborted", None, None),
            node("e", "running", Some(2), None),
            node("f", "not_started", None, None),
        ],
    );
    let p = compute(&r, None, t0() + Duration::seconds(3));
    assert_eq!((p.done, p.total), (4, 6));
    assert_eq!(p.current_step.as_deref(), Some("e"));
    assert_eq!(p.fraction, Some(4.0 / 6.0));
    assert_eq!(p.eta_secs, None);
}

#[test]
fn no_eta_below_three_runs() {
    let r = run("running", vec![node("a", "running", Some(0), None), node("b", "not_started", None, None)]);
    for n in [0, 2] {
        let p = compute(&r, Some(&past(n)), t0() + Duration::seconds(4));
        assert_eq!(p.eta_secs, None, "{n} past runs");
        assert_eq!(p.fraction, Some(0.0), "falls back to step count");
    }
}

#[test]
fn eta_from_medians() {
    let r = run("running", vec![node("a", "running", Some(0), None), node("b", "not_started", None, None)]);
    for n in [3, 10] {
        let p = compute(&r, Some(&past(n)), t0() + Duration::seconds(4));
        // 6 s left of a's 10, plus b's 30.
        assert_eq!(p.eta_secs, Some(36), "{n} past runs");
        assert_eq!(p.fraction, Some(4.0 / 40.0));
    }
}

#[test]
fn a_step_past_its_median_adds_no_negative_time() {
    let r = run("running", vec![node("a", "running", Some(0), None), node("b", "not_started", None, None)]);
    let p = compute(&r, Some(&past(3)), t0() + Duration::seconds(25));
    assert_eq!(p.eta_secs, Some(30));
}

#[test]
fn only_newest_ten_successes_count() {
    let mut runs = vec![run("failed", vec![node("a", "failed", Some(0), Some(999))])];
    runs.extend((0..10).map(|_| run("succeeded", vec![node("a", "succeeded", Some(0), Some(10))])));
    runs.extend((0..5).map(|_| run("succeeded", vec![node("a", "succeeded", Some(0), Some(500))])));
    let h = History::from_runs(&runs);
    assert_eq!(h.runs, 10);
    assert_eq!(h.medians["a"], 10.0);
}

#[test]
fn one_step_with_no_history_is_indeterminate() {
    let r = run("running", vec![node("a", "running", Some(0), None)]);
    let p = compute(&r, None, t0());
    assert_eq!(p.fraction, None);
    assert_eq!(p.eta_secs, None);
}

#[test]
fn a_step_missing_from_history_gives_no_eta() {
    let r = run("running", vec![node("a", "running", Some(0), None), node("new", "not_started", None, None)]);
    let p = compute(&r, Some(&past(5)), t0() + Duration::seconds(1));
    assert_eq!(p.eta_secs, None);
}

#[test]
fn backoff_doubles_and_caps() {
    let mut b = Backoff::default();
    assert_eq!(b.delay(), None);
    let mut seen = vec![];
    for _ in 0..9 {
        b.record_failure();
        seen.push(b.delay().unwrap().as_secs());
    }
    assert_eq!(seen, [5, 10, 20, 40, 80, 160, 300, 300, 300]);
    b.reset();
    assert_eq!(b.delay(), None);
}

#[test]
fn poll_intervals() {
    assert_eq!(poll_interval(true, false).as_secs(), 5);
    assert_eq!(poll_interval(false, true).as_secs(), 15);
    assert_eq!(poll_interval(false, false).as_secs(), 60);
}

#[test]
fn hide_rules_match_name_and_tag() {
    let cfg = Config { hidden_names: vec!["noisy".into()], hidden_tags: vec!["health".into()], ..Default::default() };
    let r = |name: &str, tag: &str| DagRun {
        name: name.into(),
        dag_run_id: "1".into(),
        status: RunStatus::Running,
        started_at: None,
        finished_at: None,
        queued_at: None,
        labels: vec![tag.into()],
        tags: vec![],
    };
    assert!(cfg.is_hidden(&r("noisy", "x")));
    assert!(cfg.is_hidden(&r("other", "health")));
    assert!(!cfg.is_hidden(&r("other", "x")));
}

#[tokio::test]
async fn an_unreachable_server_is_not_zero_running() {
    // Port 9 on localhost: nothing listens, the connection is refused.
    let c = Client::new("http://127.0.0.1:9", Auth::None);
    let s = fetch_snapshot(&c, t0(), t0()).await;
    assert!(matches!(s.connection, Connection::Unreachable(_)), "got {:?}", s.connection);
}
