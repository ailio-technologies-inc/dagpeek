// src/index.js with a stub assets binding, and the pages it serves under the CSP in public/_headers.
// Run with `pnpm test:site` (node --test, no dependencies).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import worker from '../src/index.js';

const PUBLIC = new URL('../public/', import.meta.url);
const NOT_FOUND = readFileSync(new URL('404.html', PUBLIC), 'utf8');

// Like the assets binding with not_found_handling "404-page": /404 is an ordinary page (200),
// /404.html redirects to it, and an unknown path gets the same page with 404.
const ASSETS = {
  requests: [],
  async fetch(request) {
    this.requests.push(request);
    const { pathname } = new URL(request.url);
    if (pathname === '/404.html') return new Response(null, { status: 307, headers: { location: '/404' } });
    if (pathname === '/404' && request.headers.has('if-none-match')) return new Response(null, { status: 304 });
    const status = pathname === '/404' ? 200 : 404;
    return new Response(NOT_FOUND, { status, headers: { 'content-type': 'text/html; charset=utf-8', etag: '"a"' } });
  },
};

const get = (path, headers = {}) => worker.fetch(new Request(`https://dagpeek.com${path}`, { headers }), { ASSETS });

test('/404 by name answers 404 with the not-found page, like an unknown path', async () => {
  const named = await get('/404');
  assert.equal(named.status, 404);
  assert.equal(named.headers.get('content-type'), 'text/html; charset=utf-8');
  assert.equal(await named.text(), NOT_FOUND);
  assert.equal((await get('/no-such-page')).status, 404);
});

test('/404 never answers 304, and /404.html still redirects to it', async () => {
  assert.equal((await get('/404', { 'if-none-match': '"a"' })).status, 404);
  assert.equal(ASSETS.requests.at(-1).headers.has('if-none-match'), false);
  const html = await get('/404.html');
  assert.equal(html.status, 307);
  assert.equal(html.headers.get('location'), '/404');
});

test('no page has an inline style, which the CSP (style-src \'self\') refuses', () => {
  assert.match(readFileSync(new URL('_headers', PUBLIC), 'utf8'), /style-src 'self';/);
  for (const name of readdirSync(PUBLIC).filter((f) => f.endsWith('.html'))) {
    const html = readFileSync(new URL(name, PUBLIC), 'utf8');
    assert.doesNotMatch(html, /\sstyle\s*=/i, `${name} has a style attribute`);
    assert.doesNotMatch(html, /<style[\s>]/i, `${name} has a <style> element`);
  }
});

test('the not-found page\'s classes are in site.css', () => {
  const css = readFileSync(new URL('site.css', PUBLIC), 'utf8');
  assert.match(NOT_FOUND, /<main class="not-found">/);
  assert.match(css, /\.not-found h1 \{[^}]*font-size: 44px/);
  assert.match(css, /\.not-found p \{[^}]*margin-top: 12px/);
});
