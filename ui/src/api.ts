import { invoke } from '@tauri-apps/api/core';

export type AuthKind = 'none' | 'basic' | 'bearer';
export type MenuBarStyle = 'icon' | 'failures' | 'progress' | 'words';

export interface Server {
  id: string;
  name: string;
  url: string;
  auth: AuthKind;
  username?: string | null;
}

export interface Config {
  servers: Server[];
  activeServer: string | null;
  hiddenNames: string[];
  hiddenTags: string[];
  notifyFailure: boolean;
  notifyCompletion: boolean;
  menuBarStyle: MenuBarStyle;
}

export interface DagRun {
  name: string;
  dagRunId: string;
  statusLabel: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface Progress {
  done: number;
  total: number;
  currentStep: string | null;
  fraction: number | null;
  etaSecs: number | null;
}

export interface StepNode {
  step: { name: string };
  statusLabel: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface RunView {
  run: DagRun;
  progress: Progress | null;
  steps: StepNode[];
  url: string;
}

export interface Worker {
  id: string;
  healthStatus: string;
  busyPollers: number;
  totalPollers: number;
}

export type Connection =
  | { state: 'ok' }
  | { state: 'unreachable'; detail: string }
  | { state: 'authFailed' }
  | { state: 'error'; detail: string };

export interface View {
  hasServer: boolean;
  serverName: string | null;
  serverUrl: string | null;
  connection: Connection;
  version: string | null;
  failed: RunView[];
  running: RunView[];
  queued: { name: string; items: RunView[] }[];
  workers: Worker[];
  takenAt: string;
}

export const api = {
  view: () => invoke<View>('get_view'),
  config: () => invoke<Config>('get_config'),
  savePrefs: (prefs: Pick<Config, 'hiddenNames' | 'hiddenTags' | 'notifyFailure' | 'notifyCompletion' | 'menuBarStyle'>) =>
    invoke<void>('save_prefs', { prefs }),
  saveServer: (server: Server, secret: string | null) => invoke<void>('save_server', { server, secret }),
  deleteServer: (id: string) => invoke<void>('delete_server', { id }),
  setActive: (id: string) => invoke<void>('set_active_server', { id }),
  dismissFailures: () => invoke<void>('dismiss_failures'),
  dismissFailure: (name: string) => invoke<void>('dismiss_failure', { name }),
  testServer: (url: string, kind: AuthKind, username: string | null, secret: string | null) =>
    invoke<string>('test_server', { url, kind, username, secret }),
  open: (url: string) => invoke<void>('open_url', { url }),
  checkUpdates: () => invoke<string>('check_updates'),
  quit: () => invoke<void>('quit'),
};

export function formatDuration(totalSecs: number): string {
  const s = Math.max(0, Math.round(totalSecs));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** "Just now", "13m ago", "2h ago", "3d ago": seconds would only make the list flicker. */
export function formatAgo(secs: number): string {
  if (secs < 60) return 'Just now';
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  return h < 24 ? `${h}h ago` : `${Math.floor(h / 24)}d ago`;
}

export function secondsBetween(from: string | null, to: string | null, nowMs: number): number | null {
  if (!from) return null;
  return ((to ? Date.parse(to) : nowMs) - Date.parse(from)) / 1000;
}
