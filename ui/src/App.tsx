import { listen } from '@tauri-apps/api/event';
import { LogicalSize, getCurrentWindow } from '@tauri-apps/api/window';
import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { api, formatAgo, formatDuration, secondsBetween, type RunAction, type RunView, type View } from './api';
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

function Icon({ d, box = 16 }: { d: string; box?: number }) {
  return <svg className="icon" viewBox={`0 0 ${box} ${box}`} aria-hidden="true"><path d={d} /></svg>;
}
const OPEN = 'M6.5 3H3.5A1 1 0 0 0 2.5 4v8.5a1 1 0 0 0 1 1H12a1 1 0 0 0 1-1v-3M9.5 2.5h4v4M13.5 2.5 7.5 8.5';
// Lucide's settings icon (ISC licence), on a 24 unit grid.
const GEAR = 'M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z';
const POWER = 'M8 2v5.5M11.6 4.2a5.2 5.2 0 1 1-7.2 0';
const CLOSE = 'M4 4l8 8M12 4l-8 8';
const STOP = 'M4.5 4.5h7v7h-7z';
const RETRY = 'M13 8a5 5 0 1 1-1.5-3.5M13 2.5v3h-3';

const ACTIONS: Record<RunAction, { verb: string; icon: string; question: (name: string) => string }> = {
  stop: { verb: 'Stop', icon: STOP, question: (n) => `Stop ${n}?` },
  retry: { verb: 'Retry', icon: RETRY, question: (n) => `Retry ${n}?` },
  dequeue: { verb: 'Remove', icon: CLOSE, question: (n) => `Remove ${n} from the queue?` },
};

/** An action button and the confirmation it asks for. Nothing happens on the first click. */
function useRunAction(item: RunView, action: RunAction, enabled: boolean): { button: ReactNode; bar: ReactNode } {
  const [phase, setPhase] = useState<'idle' | 'confirm' | 'busy'>('idle');
  const [error, setError] = useState('');
  if (!enabled) return { button: null, bar: null };
  const a = ACTIONS[action];

  const go = async () => {
    setPhase('busy');
    setError('');
    try {
      await api.runAction(item.serverId, action, item.run.name, item.run.dagRunId);
      setPhase('idle');
    } catch (e) {
      setError(String(e));
      setPhase('idle');
    }
  };

  return {
    button: (
      <button className="icon-btn" title={a.verb} aria-label={`${a.verb} ${item.run.name}`} onClick={() => { setError(''); setPhase('confirm'); }}>
        <Icon d={a.icon} />
      </button>
    ),
    bar:
      phase !== 'idle' || error ? (
        <div className="confirm">
          {error ? (
            <>
              <span className="error">{error}</span>
              <button className="text" onClick={() => setError('')}>OK</button>
            </>
          ) : (
            <>
              <span className="question">{a.question(item.run.name)}</span>
              <button className="text" disabled={phase === 'busy'} onClick={() => setPhase('idle')}>Cancel</button>
              <button className="act" disabled={phase === 'busy'} onClick={() => void go()}>{phase === 'busy' ? 'Working…' : a.verb}</button>
            </>
          )}
        </div>
      ) : null,
  };
}

function OpenButton({ url, name }: { url: string; name: string }) {
  return (
    <button className="icon-btn" title="Open in Dagu" aria-label={`Open ${name} in Dagu`} onClick={() => void api.open(url)}>
      <Icon d={OPEN} />
    </button>
  );
}

const ServerTag = ({ view, name }: { view: View; name: string }) => (view.allServers ? <span className="server-tag">{name}</span> : null);

function Bar({ fraction }: { fraction: number | null }) {
  return (
    <div className={fraction === null ? 'bar indeterminate' : 'bar'} role="progressbar">
      {fraction !== null && <span style={{ width: `${Math.round(fraction * 100)}%` }} />}
    </div>
  );
}

function Running({ item, view, now }: { item: RunView; view: View; now: number }) {
  const [open, setOpen] = useState(false);
  const stop = useRunAction(item, 'stop', view.allowActions);
  const p = item.progress;
  const elapsed = secondsBetween(item.run.startedAt, null, now);
  return (
    <li className="card">
      <div className="card-top">
        <button className="card-head" onClick={() => setOpen(!open)} aria-expanded={open}>
          <span className="name">{item.run.name}</span>
          <ServerTag view={view} name={item.serverName} />
          <span className="meta">{elapsed !== null && formatDuration(elapsed)}</span>
          <Chevron open={open} />
        </button>
        <span className="hover-actions">
          <OpenButton url={item.url} name={item.run.name} />
          {stop.button}
        </span>
      </div>
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
      {stop.bar}
      {open && (
        <ul className="steps">
          {item.steps.map((n) => {
            const d = secondsBetween(n.startedAt, n.finishedAt, now);
            return (
              <li key={n.step.name}>
                <button className="step-link" title="Open this step in Dagu" onClick={() => void api.open(`${item.url}?step=${encodeURIComponent(n.step.name)}`)}>
                  <i className={`dot s-${n.statusLabel}`} />
                  <span className="name">{n.step.name}</span>
                  <span className="meta">{d !== null ? formatDuration(d) : ''}</span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </li>
  );
}

function FailedGroup({ runs, view, now }: { runs: RunView[]; view: View; now: number }) {
  const [open, setOpen] = useState(false);
  const latest = runs[0]!;
  const name = latest.run.name;
  const retry = useRunAction(latest, 'retry', view.allowActions);
  const many = runs.length > 1;
  const ago = (r: RunView) => (r.run.finishedAt ? formatAgo(secondsBetween(r.run.finishedAt, null, now) ?? 0) : '');
  return (
    <li className="group">
      <div className="row">
        <button className="row-main" onClick={() => (many ? setOpen(!open) : void api.open(latest.url))} aria-expanded={many ? open : undefined}>
          <i className="dot s-failed" />
          <span className="name">{name}</span>
          {many && <span className="count">{runs.length}</span>}
          <ServerTag view={view} name={latest.serverName} />
          <span className="meta">{ago(latest)}</span>
        </button>
        <span className="hover-actions">
          <OpenButton url={latest.url} name={name} />
          {retry.button}
          <button className="icon-btn" title="Dismiss" aria-label={`Dismiss ${name}`} onClick={() => void api.dismissFailure(name)}>
            <Icon d={CLOSE} />
          </button>
        </span>
      </div>
      {retry.bar}
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

function Queued({ item, position, view }: { item: RunView; position: number; view: View }) {
  const remove = useRunAction(item, 'dequeue', view.allowActions);
  return (
    <li>
      <div className="row">
        <span className="row-main">
          <span className="name">{item.run.name}</span>
          <span className="meta">#{position}</span>
        </span>
        <span className="hover-actions">
          <OpenButton url={item.url} name={item.run.name} />
          {remove.button}
        </span>
      </div>
      {remove.bar}
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
    for (const f of view.failed) {
      const key = `${f.serverId}/${f.run.name}`;
      groups.set(key, [...(groups.get(key) ?? []), f]);
    }
    return [...groups.entries()];
  }, [view.failed]);
  const queuedCount = view.queued.reduce((n, g) => n + g.items.length, 0);
  const nothing = view.failed.length === 0 && view.running.length === 0 && queuedCount === 0;
  const busy = view.workers.reduce((n, w) => n + w.busyPollers, 0);
  const total = view.workers.reduce((n, w) => n + w.totalPollers, 0);
  return (
    <div className="board">
      {view.problems.map((p) => (
        <div className="problem" key={p.serverName}>
          <i className="dot s-failed" />
          {p.serverName}: {p.connection.state === 'authFailed' ? 'auth failed' : "can't be reached"}
        </div>
      ))}
      {nothing && view.problems.length === 0 && (
        <div className="state quiet">
          <i className="state-dot" />
          <h2>All quiet</h2>
          <p>Nothing running, queued or newly failed.</p>
        </div>
      )}
      {view.running.length > 0 && (
        <section>
          <header><h3>Running</h3><span className="meta">{view.running.length}</span></header>
          <ul className="cards">{view.running.map((r) => <Running key={`${r.serverId}/${r.run.dagRunId}`} item={r} view={view} now={now} />)}</ul>
        </section>
      )}
      {view.queued.map((g) => (
        <section key={g.name}>
          <header><h3>Queued in {g.name}</h3><span className="meta">{g.items.length}</span></header>
          <ol>{g.items.map((q, i) => <Queued key={`${q.serverId}/${q.run.dagRunId}`} item={q} position={i + 1} view={view} />)}</ol>
        </section>
      ))}
      {failedGroups.length > 0 && (
        <section>
          <header>
            <h3>Failed</h3>
            <button className="text" onClick={() => void api.dismissFailures()}>Dismiss all</button>
          </header>
          <ul>{failedGroups.map(([key, runs]) => <FailedGroup key={key} runs={runs} view={view} now={now} />)}</ul>
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

function ServerMenu({ view, onPick }: { view: View; onPick: () => void }) {
  const pick = (id: string) => { void api.setActive(id); onPick(); };
  return (
    <div className="server-menu" role="menu">
      <button role="menuitemradio" aria-checked={view.allServers} onClick={() => pick('*')}>
        <span>All servers</span>{view.allServers && <span className="tick">✓</span>}
      </button>
      {view.servers.map((s) => (
        <button key={s.id} role="menuitemradio" aria-checked={s.active} onClick={() => pick(s.id)}>
          <span>{s.name}</span>{s.active && <span className="tick">✓</span>}
        </button>
      ))}
    </div>
  );
}

export function App() {
  const [view, setView] = useState<View | null>(null);
  const [settings, setSettings] = useState(false);
  const [switching, setSwitching] = useState(false);
  const now = useNow();
  const fit = useFitWindow();

  useEffect(() => {
    void api.view().then(setView);
    const off = listen<View>('view', (e) => setView(e.payload));
    return () => void off.then((f) => f());
  }, []);

  const showSettings = view && (settings || !view.hasServer);
  const ok = view?.connection.state === 'ok';
  const canSwitch = (view?.servers.length ?? 0) > 1;

  return (
    <main className="popover" ref={fit}>
      {showSettings && <Settings firstRun={!view.hasServer} onDone={() => setSettings(false)} />}
      {view && !showSettings && (
        <>
          {ok ? <Board view={view} now={now} /> : <Unknown view={view} />}
          {switching && canSwitch && <ServerMenu view={view} onPick={() => setSwitching(false)} />}
          <footer>
            {canSwitch ? (
              <button className="server switcher" title="Switch server" aria-expanded={switching} onClick={() => setSwitching(!switching)}>
                <i className={ok ? 'dot s-succeeded' : 'dot s-failed'} />
                <span className="server-name">{view.serverName}</span>
                <Chevron open={switching} />
              </button>
            ) : (
              <span className="server" title={view.serverUrl ?? ''}>
                <i className={ok ? 'dot s-succeeded' : 'dot s-failed'} />
                {view.serverName}
              </span>
            )}
            <span className="actions">
              {view.serverUrl && (
                <button className="icon-btn" title="Open Dagu" aria-label="Open Dagu" onClick={() => void api.open(view.serverUrl!)}><Icon d={OPEN} /></button>
              )}
              <button className="icon-btn" title="Settings" aria-label="Settings" onClick={() => setSettings(true)}><Icon d={GEAR} box={24} /></button>
              <button className="icon-btn" title="Quit Dagpeek" aria-label="Quit Dagpeek" onClick={() => void api.quit()}><Icon d={POWER} /></button>
            </span>
          </footer>
        </>
      )}
    </main>
  );
}
