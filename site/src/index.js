// www goes to the bare domain; everything else is a static asset.
export default {
  fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname === 'www.dagpeek.com') {
      url.hostname = 'dagpeek.com';
      return Response.redirect(url.toString(), 301);
    }
    return env.ASSETS.fetch(request);
  },
};
