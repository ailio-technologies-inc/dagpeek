import { listen } from '@tauri-apps/api/event';
import { LogicalSize, getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { api, formatAgo, formatDuration, secondsBetween, type RunView, type View } from './api';
import { Settings } from './Settings';

const WIDTH = 380;
const MAX_HEIGHT = 620;

function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return now;
}

/** Keeps the window the height of what's in it, like a native popover. */
function useFitWindow() {
  const ref = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let last = 0;
    const fit = () => {
      const height = Math.min(Math.ceil(el.getBoundingClientRect().height), MAX_HEIGHT);
      if (height === last || height === 0) return;
      last = height;
      void getCurrentWindow().setSize(new LogicalSize(WIDTH, height));
    };
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    fit();
    return () => observer.disconnect();
  }, []);
  return ref;
}

const Chevron = ({ open }: { open: boolean }) => (
  <svg className={open ? 'chev open' : 'chev'} viewBox="0 0 12 12" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.5 3.5" /></svg>
);

function Icon({ d }: { d: string }) {
  return <svg className="icon" viewBox="0 0 16 16" aria-hidden="true"><path d={d} /></svg>;
}
const OPEN = 'M6.5 3H3.5A1 1 0 0 0 2.5 4v8.5a1 1 0 0 0 1 1H12a1 1 0 0 0 1-1v-3M9.5 2.5h4v4M13.5 2.5 7.5 8.5';
const GEAR = 'M8 10.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4ZM8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2';
const POWER = 'M8 2v5.5M11.6 4.2a5.2 5.2 0 1 1-7.2 0';
const CLOSE = 'M4 4l8 8M12 4l-8 8';

function Bar({ fraction }: { fraction: number | null }) {
  return (
    <div className={fraction === null ? 'bar indeterminate' : 'bar'} role="progressbar">
      {fraction !== null && <span style={{ width: `${Math.round(fraction * 100)}%` }} />}
    </div>
  );
}

function Running({ item, now }: { item: RunView; now: number }) {
  const [open, setOpen] = useState(false);
  const p = item.progress;
  const elapsed = secondsBetween(item.run.startedAt, null, now);
  return (
    <li className="card">
      <button className="card-head" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="name">{item.run.name}</span>
        <span className="meta">{elapsed !== null && formatDuration(elapsed)}</span>
        <Chevron open={open} />
      </button>
      <Bar fraction={p ? p.fraction : null} />
      {p && (
        <div className="detail">
          <span className="step">{p.currentStep ?? 'Starting'}</span>
          <span className="meta">
            {p.done} of {p.total}
            {p.etaSecs !== null && `, about ${formatDuration(p.etaSecs)} left`}
          </span>
        </div>
      )}
      {open && (
        <ul className="steps">
          {item.steps.map((n) => {
            const d = secondsBetween(n.startedAt, n.finishedAt, now);
            return (
              <li key={n.step.name}>
                <i className={`dot s-${n.statusLabel}`} />
                <span className="name">{n.step.name}</span>
                <span className="meta">{d !== null ? formatDuration(d) : ''}</span>
              </li>
            );
          })}
          <li className="open-link">
            <button className="text" onClick={() => void api.open(item.url)}>Open in Dagu</button>
          </li>
        </ul>
      )}
    </li>
  );
}

function FailedGroup({ name, runs, now }: { name: string; runs: RunView[]; now: number }) {
  const [open, setOpen] = useState(false);
  const latest = runs[0]!;
  const many = runs.length > 1;
  const ago = (r: RunView) => (r.run.finishedAt ? formatAgo(secondsBetween(r.run.finishedAt, null, now) ?? 0) : '');
  return (
    <li className="group">
      <div className="row">
        <button
          className="row-main"
          onClick={() => (many ? setOpen(!open) : void api.open(latest.url))}
          aria-expanded={many ? open : undefined}
        >
          <i className="dot s-failed" />
          <span className="name">{name}</span>
          {many && <span className="count">{runs.length}</span>}
          <span className="meta">{ago(latest)}</span>
        </button>
        <button className="icon-btn dismiss" title="Dismiss" aria-label={`Dismiss ${name}`} onClick={() => void api.dismissFailure(name)}>
          <Icon d={CLOSE} />
        </button>
      </div>
      {open && (
        <ul className="runs">
          {runs.map((r) => (
            <li key={r.run.dagRunId}>
              <button className="run-link" onClick={() => void api.open(r.url)}>
                <span>{ago(r)}</span>
                <span className="open-hint">Open in Dagu</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function Unknown({ view }: { view: View }) {
  const c = view.connection;
  const name = view.serverName ?? 'This server';
  return (
    <div className="state">
      <i className="state-ring" />
      {c.state === 'authFailed' ? (
        <>
          <h2>Auth failed</h2>
          <p>{name} refused the saved credentials. Update them in Settings.</p>
        </>
      ) : (
        <>
          <h2>{c.state === 'unreachable' ? 'Server unreachable' : 'Server error'}</h2>
          <p>Dagpeek can't say what {name} is doing right now. It will keep trying.</p>
        </>
      )}
    </div>
  );
}

function Board({ view, now }: { view: View; now: number }) {
  const [showWorkers, setShowWorkers] = useState(false);
  const failedGroups = useMemo(() => {
    const groups = new Map<string, RunView[]>();
    for (const f of view.failed) groups.set(f.run.name, [...(groups.get(f.run.name) ?? []), f]);
    return [...groups.entries()];
  }, [view.failed]);
  const queuedCount = view.queued.reduce((n, g) => n + g.items.length, 0);
  const nothing = view.failed.length === 0 && view.running.length === 0 && queuedCount === 0;
  const busy = view.workers.reduce((n, w) => n + w.busyPollers, 0);
  const total = view.workers.reduce((n, w) => n + w.totalPollers, 0);
  return (
    <div className="board">
      {nothing && (
        <div className="state quiet">
          <i className="state-dot" />
          <h2>All quiet</h2>
          <p>Nothing running, queued or newly failed.</p>
        </div>
      )}
      {view.running.length > 0 && (
        <section>
          <header><h3>Running</h3><span className="meta">{view.running.length}</span></header>
          <ul className="cards">{view.running.map((r) => <Running key={r.run.dagRunId} item={r} now={now} />)}</ul>
        </section>
      )}
      {view.queued.map((g) => (
        <section key={g.name}>
          <header><h3>Queued in {g.name}</h3><span className="meta">{g.items.length}</span></header>
          <ol>
            {g.items.map((q, i) => (
              <li key={q.run.dagRunId} className="plain">
                <span className="name">{q.run.name}</span>
                <span className="meta">#{i + 1}</span>
              </li>
            ))}
          </ol>
        </section>
      ))}
      {failedGroups.length > 0 && (
        <section>
          <header>
            <h3>Failed</h3>
            <button className="text" onClick={() => void api.dismissFailures()}>Dismiss all</button>
          </header>
          <ul>{failedGroups.map(([name, runs]) => <FailedGroup key={name} name={name} runs={runs} now={now} />)}</ul>
        </section>
      )}
      {view.workers.length > 0 && (
        <section>
          <button className="fold" onClick={() => setShowWorkers(!showWorkers)} aria-expanded={showWorkers}>
            <h3>Workers</h3>
            <span className="meta">{busy} of {total} busy</span>
            <Chevron open={showWorkers} />
          </button>
          {showWorkers && (
            <ul>
              {view.workers.map((w) => (
                <li key={w.id} className="plain">
                  <span className="name"><i className={`dot w-${w.healthStatus}`} />{w.id}</span>
                  <span className="meta">{w.busyPollers} of {w.totalPollers}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}

export function App() {
  const [view, setView] = useState<View | null>(null);
  const [settings, setSettings] = useState(false);
  const now = useNow();
  const fit = useFitWindow();

  useEffect(() => {
    void api.view().then(setView);
    const off = listen<View>('view', (e) => setView(e.payload));
    return () => void off.then((f) => f());
  }, []);

  const showSettings = view && (settings || !view.hasServer);
  const ok = view?.connection.state === 'ok';

  return (
    <main className="popover" ref={fit}>
      {showSettings && <Settings firstRun={!view.hasServer} onDone={() => setSettings(false)} />}
      {view && !showSettings && (
        <>
          {ok ? <Board view={view} now={now} /> : <Unknown view={view} />}
          <footer>
            <span className="server" title={view.serverUrl ?? ''}>
              <i className={ok ? 'dot s-succeeded' : 'dot s-failed'} />
              {view.serverName}
            </span>
            <span className="actions">
              {view.serverUrl && (
                <button className="icon-btn" title="Open Dagu" aria-label="Open Dagu" onClick={() => void api.open(view.serverUrl!)}><Icon d={OPEN} /></button>
              )}
              <button className="icon-btn" title="Settings" aria-label="Settings" onClick={() => setSettings(true)}><Icon d={GEAR} /></button>
              <button className="icon-btn" title="Quit Dagpeek" aria-label="Quit Dagpeek" onClick={() => void api.quit()}><Icon d={POWER} /></button>
            </span>
          </footer>
        </>
      )}
    </main>
  );
}
