// The www redirect, the consent answer, and static assets.
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

export default {
  fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname === 'www.dagpeek.com') {
      url.hostname = 'dagpeek.com';
      return Response.redirect(url.toString(), 301);
    }
    if (url.pathname === '/data/consent') return consentRoute(request);
    return env.ASSETS.fetch(request);
  },
};
