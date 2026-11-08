// Google Analytics for this website only, and only after a visitor says yes. The app has none.
(function () {
  'use strict';

  var ID = 'G-GQ102DTLE4';
  var KEY = 'dagpeek_consent';
  var banner = document.getElementById('consent');
  var started = false;

  function read() {
    try { return localStorage.getItem(KEY); } catch (e) { return null; }
  }

  function write(value) {
    try { localStorage.setItem(KEY, value); } catch (e) {}
  }

  function start() {
    if (started) return;
    started = true;
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
    write(choice);
    banner.hidden = true;
    if (choice === 'granted') {
      start();
    } else {
      var wasOn = started;
      forget();
      if (wasOn) location.reload();
    }
  });

  document.getElementById('cookie-choices').addEventListener('click', function () {
    banner.hidden = false;
  });

  document.addEventListener('click', function (event) {
    var link = event.target.closest && event.target.closest('a.button');
    if (link && window.gtag) window.gtag('event', 'download_click');
  });

  var answer = read();
  if (answer === 'granted') start();
  else if (answer === null) banner.hidden = false;
})();
