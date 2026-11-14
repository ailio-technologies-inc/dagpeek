// The www redirect, the consent answer, the release line, and static assets.
//
// Consent is the same model as Ailio's other sites: analytics is opt-out, except that Quebec
// (Law 25) waits for "Allow". The question shows only to a reader who has never answered.
const COOKIE = 'dagpeek_consent';
const YEAR = 365 * 24 * 60 * 60;
const NO_STORE = { 'cache-control': 'private, no-store', 'content-type': 'application/json; charset=utf-8' };

function readConsent(header) {
  for (const part of (header ?? '').split(';')) {
    const eq = part.indexOf('=');
    if (eq < 0 || part.slice(0, eq).trim() !== COOKIE) continue;
    const value = part.slice(eq + 1).trim();
    return value === 'granted' || value === 'denied' ? value : 'pending';
  }
  return 'pending';
}

async function consentRoute(request) {
  if (request.method === 'POST') {
    const body = await request.json().catch(() => null);
    if (body?.choice !== 'granted' && body?.choice !== 'denied') {
      return new Response('{"error":"choice must be granted or denied"}', { status: 400, headers: NO_STORE });
    }
    const cookie = `${COOKIE}=${body.choice}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=${YEAR}`;
    return new Response('{"ok":true}', { headers: { ...NO_STORE, 'set-cookie': cookie } });
  }
  if (request.method !== 'GET') return new Response(null, { status: 405, headers: { allow: 'GET, POST' } });
  const consent = readConsent(request.headers.get('cookie'));
  const quebec = request.cf?.country?.toUpperCase() === 'CA' && request.cf?.regionCode?.toUpperCase() === 'QC';
  const analytics = consent === 'granted' || (consent === 'pending' && !quebec);
  return new Response(JSON.stringify({ consent, analytics, banner: consent === 'pending' }), { headers: NO_STORE });
}

// The home page's version line and JSON-LD softwareVersion come from the app's own updater feed,
// which scripts/release.sh writes for every release, so the page never needs a version bump.
const FEED = 'https://github.com/ailio-technologies-inc/dagpeek/releases/latest/download/latest.json';
const FRESH_MS = 10 * 60 * 1000;
const RETRY_MS = 60 * 1000;
let memo = { at: 0, ttl: 0, release: null };

async function latestRelease() {
  if (Date.now() - memo.at < memo.ttl) return memo.release;
  try {
    const response = await fetch(FEED, { cf: { cacheTtl: 600, cacheEverything: true }, signal: AbortSignal.timeout(2000) });
    if (!response.ok) throw new Error(`feed ${response.status}`);
    const feed = await response.json();
    if (typeof feed.version !== 'string' || !/^\d+\.\d+\.\d+$/.test(feed.version)) throw new Error('feed has no version');
    const date = new Date(feed.pub_date);
    const release = { version: feed.version, date: Number.isNaN(date.getTime()) ? null : date };
    memo = { at: Date.now(), ttl: FRESH_MS, release };
  } catch {
    // Keep the last good answer; with none, the page shows its release notes link alone.
    memo = { at: Date.now(), ttl: RETRY_MS, release: memo.release };
  }
  return memo.release;
}

function releaseText(release) {
  if (!release.date) return `Version ${release.version}. `;
  const iso = release.date.toISOString().slice(0, 10);
  const day = release.date.toLocaleDateString('en-US', { timeZone: 'UTC', month: 'long', day: 'numeric', year: 'numeric' });
  return `Version ${release.version}, released <time datetime="${iso}">${day}</time>. `;
}

// The JSON-LD can arrive in several text chunks, so collect it and write it back once.
class JsonLd {
  constructor(version) { this.version = version; this.buffer = ''; }
  text(chunk) {
    this.buffer += chunk.text;
    if (!chunk.lastInTextNode) { chunk.remove(); return; }
    try {
      const data = JSON.parse(this.buffer);
      data.softwareVersion = this.version;
      chunk.replace(`\n${JSON.stringify(data)}\n`, { html: true });
    } catch {
      chunk.replace(this.buffer, { html: true });
    }
  }
}

async function homePage(request, env) {
  // No conditional request: the body changes with each release, not with the asset's ETag.
  const headers = new Headers(request.headers);
  headers.delete('if-none-match');
  headers.delete('if-modified-since');
  const [page, release] = await Promise.all([env.ASSETS.fetch(new Request(request, { headers })), latestRelease()]);
  if (!release || page.status !== 200 || !(page.headers.get('content-type') ?? '').startsWith('text/html')) return page;
  const response = new HTMLRewriter()
    .on('[data-release]', { element(el) { el.setInnerContent(releaseText(release), { html: true }); } })
    .on('script[type="application/ld+json"]', new JsonLd(release.version))
    .transform(page);
  const out = new Response(response.body, response);
  out.headers.delete('etag');
  out.headers.delete('content-length');
  out.headers.set('cache-control', 'public, max-age=0, must-revalidate');
  return out;
}

export default {
  fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname === 'www.dagpeek.com') {
      url.hostname = 'dagpeek.com';
      return Response.redirect(url.toString(), 301);
    }
    if (url.pathname === '/data/consent') return consentRoute(request);
    if (url.pathname === '/' && (request.method === 'GET' || request.method === 'HEAD')) return homePage(request, env);
    return env.ASSETS.fetch(request);
  },
};
