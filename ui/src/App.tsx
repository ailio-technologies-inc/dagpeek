import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { api, formatDuration, secondsBetween, type RunView, type View } from './api';
import { Settings } from './Settings';

function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return now;
}

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
    <li className="run">
      <button className="row" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="name">{item.run.name}</span>
        <span className="meta">{elapsed !== null && formatDuration(elapsed)}</span>
      </button>
      <Bar fraction={p ? p.fraction : null} />
      {p && (
        <div className="detail">
          <span className="step">{p.currentStep ?? 'Starting'}</span>
          <span>
            {p.done} of {p.total} steps
            {p.etaSecs !== null && ` · about ${formatDuration(p.etaSecs)} left`}
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
          <li>
            <button className="link" onClick={() => void api.open(item.url)}>Open in Dagu</button>
          </li>
        </ul>
      )}
    </li>
  );
}

function Unknown({ view }: { view: View }) {
  const c = view.connection;
  if (c.state === 'authFailed') {
    return (
      <div className="state">
        <h2>Auth failed</h2>
        <p>{view.serverName ?? 'This server'} refused the saved credentials. Update them in Settings.</p>
      </div>
    );
  }
  return (
    <div className="state">
      <h2>{c.state === 'unreachable' ? 'Server unreachable' : 'Server error'}</h2>
      <p>Dagpeek can't say what {view.serverName ?? 'this server'} is doing right now. It will keep trying.</p>
    </div>
  );
}

function Board({ view, now }: { view: View; now: number }) {
  const [showWorkers, setShowWorkers] = useState(false);
  const queuedCount = view.queued.reduce((n, g) => n + g.items.length, 0);
  const nothing = view.failed.length === 0 && view.running.length === 0 && queuedCount === 0;
  const busy = view.workers.reduce((n, w) => n + w.busyPollers, 0);
  const total = view.workers.reduce((n, w) => n + w.totalPollers, 0);
  return (
    <div className="board">
      {view.failed.length > 0 && (
        <section>
          <header>
            <h3 className="failed">Failed</h3>
            <button className="link" onClick={() => void api.dismissFailures()}>Dismiss all</button>
          </header>
          <ul>
            {view.failed.map((f) => (
              <li key={f.run.dagRunId}>
                <button className="row" onClick={() => void api.open(f.url)}>
                  <span className="name">{f.run.name}</span>
                  <span className="meta">
                    {f.run.finishedAt &&
                      `${formatDuration(secondsBetween(f.run.finishedAt, null, now) ?? 0)} ago`}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {view.running.length > 0 && (
        <section>
          <header><h3>Running</h3><span className="meta">{view.running.length}</span></header>
          <ul>{view.running.map((r) => <Running key={r.run.dagRunId} item={r} now={now} />)}</ul>
        </section>
      )}
      {view.queued.map((g) => (
        <section key={g.name}>
          <header><h3>Queued: {g.name}</h3><span className="meta">{g.items.length}</span></header>
          <ol>
            {g.items.map((q, i) => (
              <li key={q.run.dagRunId} className="row static">
                <span className="name">{q.run.name}</span>
                <span className="meta">#{i + 1}</span>
              </li>
            ))}
          </ol>
        </section>
      ))}
      {nothing && (
        <div className="state">
          <h2>All quiet</h2>
          <p>Nothing running, queued or newly failed.</p>
        </div>
      )}
      {view.workers.length > 0 && (
        <section>
          <button className="row" onClick={() => setShowWorkers(!showWorkers)} aria-expanded={showWorkers}>
            <h3>Workers</h3>
            <span className="meta">{busy} of {total} busy</span>
          </button>
          {showWorkers && (
            <ul>
              {view.workers.map((w) => (
                <li key={w.id} className="row static">
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

  useEffect(() => {
    void api.view().then(setView);
    const off = listen<View>('view', (e) => setView(e.payload));
    return () => void off.then((f) => f());
  }, []);

  if (!view) return <main className="popover" />;
  if (settings || !view.hasServer) {
    return (
      <main className="popover">
        <Settings firstRun={!view.hasServer} onDone={() => setSettings(false)} />
      </main>
    );
  }

  return (
    <main className="popover">
      {view.connection.state === 'ok' ? <Board view={view} now={now} /> : <Unknown view={view} />}
      <footer>
        <span className="server" title={view.serverUrl ?? ''}>{view.serverName}</span>
        <span className="actions">
          {view.serverUrl && <button className="link" onClick={() => void api.open(view.serverUrl!)}>Open Dagu</button>}
          <button className="link" onClick={() => setSettings(true)}>Settings</button>
          <button className="link" onClick={() => void api.quit()}>Quit</button>
        </span>
      </footer>
    </main>
  );
}
