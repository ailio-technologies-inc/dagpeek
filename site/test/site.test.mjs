// public/site.js, run in a stub page: Google Analytics loads only by consent and never in one of
// our own automated browsers. Run with `pnpm test:site` (node --test, no dependencies).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const SOURCE = readFileSync(new URL('../public/site.js', import.meta.url), 'utf8');
const READER_UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/142.0 Safari/537.36';

function element() {
  return { hidden: true, addEventListener() {} };
}

// Runs site.js once and resolves with what it did: whether gtag.js was added, and the banner.
async function load({ analytics = true, webdriver = false, userAgent = READER_UA, search = '', stored = {} } = {}) {
  const scripts = [];
  const banner = element();
  const memory = new Map(Object.entries(stored));
  const window = {
    localStorage: {
      getItem: (k) => (memory.has(k) ? memory.get(k) : null),
      setItem: (k, v) => void memory.set(k, String(v)),
    },
  };
  const context = {
    window,
    navigator: { webdriver, userAgent },
    location: { search, hostname: 'dagpeek.com', reload() {} },
    URLSearchParams,
    document: {
      cookie: '',
      head: { appendChild: (node) => scripts.push(node.src) },
      createElement: () => ({}),
      getElementById: (id) => (id === 'consent' ? banner : element()),
      addEventListener() {},
    },
    fetch: async () => ({ json: async () => ({ analytics, banner: false }) }),
  };
  vm.runInNewContext(SOURCE, context);
  await new Promise((resolve) => setImmediate(resolve));
  return { gtag: scripts.some((src) => src.startsWith('https://www.googletagmanager.com/gtag/js')), memory };
}

test('a reader who has consented gets Google Analytics, and one who has not gets none', async () => {
  assert.equal((await load()).gtag, true);
  assert.equal((await load({ analytics: false })).gtag, false);
});

test('our automated browsers never load it: webdriver, headless Chrome, AilioE2E/1', async () => {
  assert.equal((await load({ webdriver: true })).gtag, false);
  assert.equal((await load({ userAgent: 'Mozilla/5.0 HeadlessChrome/142.0' })).gtag, false);
  assert.equal((await load({ userAgent: `${READER_UA} AilioE2E/1` })).gtag, false);
});

test('?bot=1 keeps it out and is remembered for later pages; only 1 counts', async () => {
  const first = await load({ search: '?bot=1' });
  assert.equal(first.gtag, false);
  assert.equal(first.memory.get('dagpeek:bot'), '1');
  assert.equal((await load({ stored: { 'dagpeek:bot': '1' } })).gtag, false, 'the next page, with no parameter');
  assert.equal((await load({ search: '?bot=0' })).gtag, true);
});
