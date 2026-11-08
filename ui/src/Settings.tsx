import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { useEffect, useState } from 'react';
import { api, type AuthKind, type Config, type Server } from './api';

const blank = (): Server => ({ id: crypto.randomUUID(), name: '', url: '', auth: 'none', username: '' });
const list = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);

function ServerForm({ initial, onDone }: { initial: Server; onDone: () => void }) {
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
      <label>Server URL<input required type="url" placeholder="http://dagu.example:8080" value={s.url} onChange={(e) => setS({ ...s, url: e.target.value })} /></label>
      <label>Name<input placeholder="Optional" value={s.name} onChange={(e) => setS({ ...s, name: e.target.value })} /></label>
      <label>Sign-in
        <select value={s.auth} onChange={(e) => setS({ ...s, auth: e.target.value as AuthKind })}>
          <option value="none">None</option>
          <option value="basic">Username and password</option>
          <option value="bearer">API token</option>
        </select>
      </label>
      {s.auth === 'basic' && <label>Username<input value={s.username ?? ''} onChange={(e) => setS({ ...s, username: e.target.value })} /></label>}
      {s.auth !== 'none' && (
        <label>{s.auth === 'basic' ? 'Password' : 'Token'}
          <input type="password" autoComplete="off" placeholder={isNew ? '' : 'Saved. Type to replace.'} value={secret} onChange={(e) => setSecret(e.target.value)} />
        </label>
      )}
      {note && <p className="note">{note}</p>}
      <div className="buttons">
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

  const load = () => api.config().then((c) => {
    setConfig(c);
    setNames(c.hiddenNames.join(', '));
    setTags(c.hiddenTags.join(', '));
  });

  useEffect(() => {
    void load();
    void isEnabled().then(setAutostart).catch(() => setAutostart(false));
  }, []);

  if (!config) return null;

  if (editing) {
    return (
      <div className="settings">
        <h2>{firstRun ? 'Add your Dagu server' : 'Server'}</h2>
        {firstRun && <p className="note">Dagpeek only talks to the servers you add here. Passwords and tokens stay in your system's credential store.</p>}
        <ServerForm initial={editing} onDone={() => { setEditing(null); void load(); }} />
        {!firstRun && <button className="link" onClick={() => setEditing(null)}>Back</button>}
      </div>
    );
  }

  const savePrefs = (next: Partial<Config>) => {
    const merged = { ...config, hiddenNames: list(names), hiddenTags: list(tags), ...next };
    setConfig(merged);
    void api.savePrefs(merged);
  };

  return (
    <div className="settings">
      <header><h2>Settings</h2><button className="link" onClick={() => { savePrefs({}); onDone(); }}>Done</button></header>

      <section>
        <h3>Servers</h3>
        <ul>
          {config.servers.map((s) => (
            <li key={s.id} className="row static">
              <label className="radio">
                <input type="radio" name="active" checked={config.activeServer === s.id} onChange={() => { void api.setActive(s.id); setConfig({ ...config, activeServer: s.id }); }} />
                <span className="name">{s.name}</span>
              </label>
              <span className="actions">
                <button className="link" onClick={() => setEditing(s)}>Edit</button>
                <button className="link" onClick={() => void api.deleteServer(s.id).then(load)}>Remove</button>
              </span>
            </li>
          ))}
        </ul>
        <button className="link" onClick={() => setEditing(blank())}>Add a server</button>
      </section>

      <section>
        <h3>Hide runs</h3>
        <label>DAG names<input placeholder="cluster-health, nightly-sync" value={names} onChange={(e) => setNames(e.target.value)} onBlur={() => savePrefs({})} /></label>
        <label>Tags<input placeholder="noisy" value={tags} onChange={(e) => setTags(e.target.value)} onBlur={() => savePrefs({})} /></label>
      </section>

      <section>
        <h3>General</h3>
        <label className="check"><input type="checkbox" checked={autostart} onChange={(e) => { setAutostart(e.target.checked); void (e.target.checked ? enable() : disable()); }} />Open at login</label>
        <label className="check"><input type="checkbox" checked={config.notifyFailure} onChange={(e) => savePrefs({ notifyFailure: e.target.checked })} />Notify me when a run fails</label>
        <label className="check"><input type="checkbox" checked={config.notifyCompletion} onChange={(e) => savePrefs({ notifyCompletion: e.target.checked })} />Notify me when a run finishes</label>
      </section>
    </div>
  );
}
