import { getVersion } from '@tauri-apps/api/app';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { useEffect, useState } from 'react';
import { api, type AuthKind, type Config, type Server } from './api';

const blank = (): Server => ({ id: crypto.randomUUID(), name: '', url: '', auth: 'none', username: '' });
const list = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="set-row toggle">
      <span>{label}</span>
      <input className="switch" type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

function ServerForm({ initial, onDone, onCancel }: { initial: Server; onDone: () => void; onCancel?: () => void }) {
  const [s, setS] = useState(initial);
  const [secret, setSecret] = useState('');
  const [note, setNote] = useState<string | null>(null);
  const isNew = initial.url === '';

  const test = async () => {
    setNote('Checking…');
    try {
      const v = await api.testServer(s.url, s.auth, s.username ?? null, secret || null);
      setNote(`Connected to Dagu ${v}`);
    } catch (e) {
      setNote(String(e));
    }
  };

  const save = async () => {
    try {
      const name = s.name.trim() || new URL(s.url).host;
      await api.saveServer({ ...s, name, url: s.url.trim() }, secret || null);
      onDone();
    } catch (e) {
      setNote(String(e));
    }
  };

  return (
    <form className="form" onSubmit={(e) => { e.preventDefault(); void save(); }}>
      <div className="set-group">
        <label className="set-row field"><span>URL</span><input required type="url" placeholder="http://dagu.example:8080" value={s.url} onChange={(e) => setS({ ...s, url: e.target.value })} /></label>
        <label className="set-row field"><span>Name</span><input placeholder="Optional" value={s.name} onChange={(e) => setS({ ...s, name: e.target.value })} /></label>
        <label className="set-row field"><span>Sign-in</span>
          <select value={s.auth} onChange={(e) => setS({ ...s, auth: e.target.value as AuthKind })}>
            <option value="none">None</option>
            <option value="basic">Username and password</option>
            <option value="bearer">API token</option>
          </select>
        </label>
        {s.auth === 'basic' && <label className="set-row field"><span>Username</span><input value={s.username ?? ''} onChange={(e) => setS({ ...s, username: e.target.value })} /></label>}
        {s.auth !== 'none' && (
          <label className="set-row field"><span>{s.auth === 'basic' ? 'Password' : 'Token'}</span>
            <input type="password" autoComplete="off" placeholder={isNew ? '' : 'Saved. Type to replace.'} value={secret} onChange={(e) => setSecret(e.target.value)} />
          </label>
        )}
      </div>
      {note && <p className="note">{note}</p>}
      <div className="buttons">
        {onCancel && <button type="button" onClick={onCancel}>Cancel</button>}
        <button type="button" onClick={() => void test()} disabled={!s.url}>Test</button>
        <button type="submit" className="primary">Save</button>
      </div>
    </form>
  );
}

export function Settings({ firstRun, onDone }: { firstRun: boolean; onDone: () => void }) {
  const [config, setConfig] = useState<Config | null>(null);
  const [editing, setEditing] = useState<Server | null>(firstRun ? blank() : null);
  const [autostart, setAutostart] = useState(false);
  const [names, setNames] = useState('');
  const [tags, setTags] = useState('');
  const [version, setVersion] = useState('');
  const [update, setUpdate] = useState<string | null>(null);

  const load = () => api.config().then((c) => {
    setConfig(c);
    setNames(c.hiddenNames.join(', '));
    setTags(c.hiddenTags.join(', '));
  });

  useEffect(() => {
    void load();
    void isEnabled().then(setAutostart).catch(() => setAutostart(false));
    void getVersion().then(setVersion).catch(() => setVersion(''));
  }, []);

  if (!config) return null;

  if (editing) {
    return (
      <div className="settings">
        <header><h2>{firstRun ? 'Add your Dagu server' : editing.url ? 'Edit server' : 'Add a server'}</h2></header>
        {firstRun && <p className="note">Dagpeek only talks to the servers you add here. Passwords and tokens stay in your system's credential store.</p>}
        <ServerForm initial={editing} onDone={() => { setEditing(null); void load(); }} onCancel={firstRun ? undefined : () => setEditing(null)} />
      </div>
    );
  }

  const savePrefs = (next: Partial<Config>) => {
    const merged = { ...config, hiddenNames: list(names), hiddenTags: list(tags), ...next };
    setConfig(merged);
    void api.savePrefs(merged);
  };

  const checkUpdates = async () => {
    setUpdate('Checking…');
    try {
      setUpdate(await api.checkUpdates());
    } catch (e) {
      setUpdate(`Couldn't check for updates: ${String(e)}`);
    }
  };

  const many = config.servers.length > 1;

  return (
    <div className="settings">
      <header><h2>Settings</h2><button className="text done" onClick={() => { savePrefs({}); onDone(); }}>Done</button></header>

      <section>
        <h3>Servers</h3>
        <div className="set-group">
          {config.servers.map((s) => (
            <div key={s.id} className="set-row server-row">
              {many && (
                <input type="radio" name="active" aria-label={`Use ${s.name}`} checked={config.activeServer === s.id} onChange={() => { void api.setActive(s.id); setConfig({ ...config, activeServer: s.id }); }} />
              )}
              <span className="server-text">
                <span className="name">{s.name}</span>
                <span className="meta">{s.url}</span>
              </span>
              <span className="actions">
                <button className="text" onClick={() => setEditing(s)}>Edit</button>
                <button className="text" onClick={() => void api.deleteServer(s.id).then(load)}>Remove</button>
              </span>
            </div>
          ))}
          <button className="set-row add" onClick={() => setEditing(blank())}>Add a server</button>
        </div>
      </section>

      <section>
        <h3>Hide runs</h3>
        <div className="set-group">
          <label className="set-row field"><span>DAGs</span><input placeholder="cluster-health, nightly-sync" value={names} onChange={(e) => setNames(e.target.value)} onBlur={() => savePrefs({})} /></label>
          <label className="set-row field"><span>Tags</span><input placeholder="noisy" value={tags} onChange={(e) => setTags(e.target.value)} onBlur={() => savePrefs({})} /></label>
        </div>
      </section>

      <section>
        <h3>General</h3>
        <div className="set-group">
          <Toggle label="Open at login" checked={autostart} onChange={(v) => { setAutostart(v); void (v ? enable() : disable()); }} />
          <Toggle label="Notify me when a run fails" checked={config.notifyFailure} onChange={(v) => savePrefs({ notifyFailure: v })} />
          <Toggle label="Notify me when a run finishes" checked={config.notifyCompletion} onChange={(v) => savePrefs({ notifyCompletion: v })} />
        </div>
      </section>

      <section>
        <h3>About</h3>
        <div className="set-group">
          <div className="set-row">
            <span>Dagpeek {version}</span>
            <button className="text" onClick={() => void checkUpdates()}>Check for updates</button>
          </div>
        </div>
        {update && <p className="note status-note">{update}</p>}
      </section>
    </div>
  );
}
