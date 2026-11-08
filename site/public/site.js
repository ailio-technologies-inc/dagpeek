// Google Analytics for this website, by the visitor's consent (src/index.js holds the rules:
// opt-out, except Quebec, which waits for Allow).
(function () {
  'use strict';

  var ID = 'G-GQ102DTLE4';
  var banner = document.getElementById('consent');

  function start() {
    window.dataLayer = window.dataLayer || [];
    window.gtag = function () { window.dataLayer.push(arguments); };
    window.gtag('consent', 'default', { ad_storage: 'denied', ad_user_data: 'denied', ad_personalization: 'denied', analytics_storage: 'granted' });
    window.gtag('js', new Date());
    window.gtag('config', ID);
    var script = document.createElement('script');
    script.async = true;
    script.src = 'https://www.googletagmanager.com/gtag/js?id=' + ID;
    document.head.appendChild(script);
  }

  function forget() {
    document.cookie.split(';').forEach(function (part) {
      var name = part.split('=')[0].trim();
      if (name.indexOf('_ga') !== 0) return;
      ['', '; domain=' + location.hostname, '; domain=.' + location.hostname].forEach(function (domain) {
        document.cookie = name + '=; Max-Age=0; path=/' + domain;
      });
    });
  }

  banner.addEventListener('click', function (event) {
    var choice = event.target.getAttribute && event.target.getAttribute('data-choice');
    if (!choice) return;
    banner.hidden = true;
    fetch('/data/consent', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ choice: choice }),
    }).then(function () {
      if (choice === 'denied') forget();
      location.reload();
    });
  });

  document.getElementById('cookie-choices').addEventListener('click', function () {
    banner.hidden = false;
  });

  document.addEventListener('click', function (event) {
    var link = event.target.closest && event.target.closest('a.button');
    if (link && window.gtag) window.gtag('event', 'download_click');
  });

  fetch('/data/consent', { credentials: 'same-origin' })
    .then(function (response) { return response.json(); })
    .then(function (state) {
      if (state.analytics) start();
      if (state.banner) banner.hidden = false;
    })
    .catch(function () {});
})();
