// Sample data for scripts/screenshot.sh. The clock is frozen, so the image only changes when the UI does.
(() => {
  const T = Date.UTC(2026, 0, 15, 12, 0, 0);
  const RealDate = Date;
  window.Date = class extends RealDate {
    constructor(...args) {
      if (args.length) super(...args);
      else super(T);
    }
    static now() {
      return T;
    }
  };
  const ago = (minutes) => new RealDate(T - minutes * 60000).toISOString();
  const failure = (name, i, minutes) => ({
    run: { name, dagRunId: `${name}-${i}`, statusLabel: 'failed', startedAt: ago(minutes + 1), finishedAt: ago(minutes) },
    progress: null,
    steps: [],
    url: 'http://dagu.internal/',
  });
  const failed = [
    ['nightly-backup', [14, 29, 44, 59, 74, 98]],
    ['sync-invoices', [27, 87, 141]],
    ['load-warehouse', [47]],
    ['weekly-summary', [61]],
    ['rotate-logs', [190, 250]],
  ]
    .flatMap(([name, times]) => times.map((m, i) => failure(name, i, m)))
    .sort((a, b) => b.run.finishedAt.localeCompare(a.run.finishedAt));
  const view = {
    hasServer: true,
    serverName: 'Production',
    serverUrl: 'http://dagu.internal',
    connection: { state: 'ok' },
    version: '2.25.1',
    failed,
    running: [
      {
        run: { name: 'deploy-web', dagRunId: 'r1', statusLabel: 'running', startedAt: ago(6.4), finishedAt: null },
        progress: { done: 3, total: 6, currentStep: 'test', fraction: 0.58, etaSecs: 330 },
        steps: [],
        url: 'http://dagu.internal/',
      },
    ],
    queued: [],
    workers: [{ id: 'worker-1', healthStatus: 'healthy', busyPollers: 1, totalPollers: 4 }],
    takenAt: ago(0),
    allServers: false,
    servers: [],
    problems: [],
    allowActions: false,
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 1,
    unregisterCallback() {},
    metadata: { currentWindow: { label: 'popover' }, currentWebview: { label: 'popover' } },
    invoke: async (command) => {
      if (command === 'get_view') return view;
      if (command === 'get_config') return { servers: [], hiddenNames: [], hiddenTags: [] };
      return 1;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
})();
