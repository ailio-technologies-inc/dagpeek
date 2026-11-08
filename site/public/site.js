// Moves the page's popover, tray icon and wallpaper to the state each section describes.
document.documentElement.classList.add('js');

const FOOT = { server: 'Production', acts: ['Open Dagu', 'Settings'] };

const deploy = { name: 'deploy-web', time: '2m 41s', pct: 67, step: 'run-migrations', text: '4 of 6 steps, about 1m 10s left' };
const load = { name: 'load-warehouse', time: '38s', pct: 20, step: 'extract', text: '1 of 5 steps' };
const queued = { title: 'Queued: reports', rows: [['weekly-summary', '#1']] };
const failedRows = { rows: [['nightly-backup', '4h ago']] };

const STATES = {
  running: { tray: { ring: 'spin' }, counts: [2, 1, 1], failed: failedRows, running: [deploy, load], queued },
  steps: {
    tray: { ring: 'part', at: 0.67 }, counts: [1, 0, 0],
    running: [{
      ...deploy,
      steps: [
        ['ok', 'checkout', '4s'], ['ok', 'build', '1m 12s'], ['ok', 'test', '58s'], ['ok', 'upload', '9s'],
        ['now', 'run-migrations', '18s'], ['', 'switch-traffic', ''],
      ],
    }],
  },
  failed: {
    tray: { ring: 'idle' }, counts: [0, 0, 2],
    failed: { rows: [['nightly-backup', '4h ago'], ['sync-invoices', '6h ago']], dismiss: true },
  },
  quiet: { tray: { ring: 'idle' }, counts: [0, 0, 0], empty: ['All quiet', 'Nothing running, queued or newly failed.'] },
  unreachable: {
    tray: { ring: 'broken' }, counts: null,
    empty: ['Server unreachable', "Dagpeek can't say what Production is doing right now. It will keep trying."],
  },
  settings: {
    tray: { ring: 'idle' }, counts: [0, 0, 0],
    fields: [['Server URL', 'http://dagu.internal:8080'], ['Sign-in', 'API token'], ['Token', 'Saved. Type to replace.']],
    note: "Saved in your system's credential store, not in a file.",
    noFoot: true,
  },
};

function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}

function row(a, b) {
  const r = el('div', 'row');
  r.append(el('span', '', a), el('span', 'meta', b));
  return r;
}

function run(section, r) {
  section.append(row(r.name, r.time));
  const bar = el('div', 'bar');
  const fill = el('i');
  fill.style.width = `${r.pct}%`;
  bar.append(fill);
  const detail = el('div', 'detail');
  detail.append(el('span', '', r.step), el('span', '', r.text));
  section.append(bar, detail);
  if (r.steps) {
    const list = el('div', 'steps');
    for (const [dot, name, time] of r.steps) {
      const line = el('div');
      line.append(el('i', `dot ${dot}`), el('span', '', name), el('span', 'meta', time));
      list.append(line);
    }
    section.append(list);
  }
}

function body(state) {
  const frag = document.createDocumentFragment();
  const main = el('div', 'pop-body');
  if (state.empty) {
    const e = el('div', 'empty');
    e.append(el('b', '', state.empty[0]), el('p', '', state.empty[1]));
    main.append(e);
  }
  if (state.fields) {
    const h = el('h3', '', 'Server');
    main.append(h);
    for (const [label, value] of state.fields) {
      const f = el('div', 'field', label);
      f.append(el('span', '', value));
      main.append(f);
    }
    main.append(el('p', 'note', state.note));
  }
  if (state.failed) {
    const s = el('section');
    const head = el('div', 'head');
    head.append(el('h3', 'bad', 'Failed'));
    if (state.failed.dismiss) head.append(el('span', 'link', 'Dismiss all'));
    s.append(head);
    state.failed.rows.forEach(([a, b]) => s.append(row(a, b)));
    main.append(s);
  }
  if (state.running) {
    const s = el('section');
    s.append(el('h3', '', 'Running'));
    state.running.forEach((r) => run(s, r));
    main.append(s);
  }
  if (state.queued) {
    const s = el('section');
    s.append(el('h3', '', state.queued.title));
    state.queued.rows.forEach(([a, b]) => s.append(row(a, b)));
    main.append(s);
  }
  frag.append(main);
  if (!state.noFoot) {
    const foot = el('div', 'foot-bar');
    const acts = el('span', 'acts');
    FOOT.acts.forEach((a) => acts.append(el('span', '', a)));
    foot.append(el('span', '', FOOT.server), acts);
    frag.append(foot);
  }
  return frag;
}

const SVG = 'http://www.w3.org/2000/svg';

function circle(attrs) {
  const c = document.createElementNS(SVG, 'circle');
  for (const [k, v] of Object.entries({ cx: 11, cy: 11, r: 8, ...attrs })) c.setAttribute(k, v);
  return c;
}

function trayIcon({ ring, at }) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('viewBox', '0 0 22 22');
  const arc = (dash) => circle({ pathLength: 100, 'stroke-dasharray': dash, transform: 'rotate(-90 11 11)' });
  if (ring === 'idle') svg.append(circle({}), circle({ r: 2.5, class: 'fill-dot' }));
  if (ring === 'spin') svg.append(circle({ class: 'track' }), arc('30 100'));
  if (ring === 'part') svg.append(circle({ class: 'track' }), arc(`${at * 100} 100`));
  if (ring === 'broken') svg.append(arc('6.25 6.25'));
  return svg;
}

const tray = document.getElementById('tray');
const pop = document.getElementById('pop');
let current = null;

function show(name) {
  if (name === current) return;
  current = name;
  const state = STATES[name];
  document.documentElement.dataset.state = name;

  tray.replaceChildren(trayIcon(state.tray));
  if (state.counts) {
    const [r, q, f] = state.counts;
    const title = el('span', 'tray-title');
    if (r) title.append(el('span', '', `▶${r}`));
    if (q) title.append(el('span', '', `⏸${q}`));
    if (f) title.append(el('span', '', `✕${f}`));
    tray.append(title);
  }
  pop.replaceChildren(body(state));
}

document.querySelectorAll('[data-mount]').forEach((mount) => {
  const p = el('div', 'mount-pop');
  p.setAttribute('aria-hidden', 'true');
  p.append(body(STATES[mount.dataset.mount]));
  mount.append(p);
});

const io = new IntersectionObserver(
  (entries) => {
    for (const e of entries) if (e.isIntersecting) show(e.target.dataset.scene);
  },
  // A thin line 30% down the screen, so the section being read decides, not the next one up.
  { rootMargin: '-30% 0px -69% 0px' },
);
document.querySelectorAll('[data-scene]').forEach((s) => io.observe(s));
show('running');
