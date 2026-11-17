// LiteRust cosmetic filter: hides only common ad elements after DOM construction.
// This does NOT block requests, trackers, scripts, analytics or cookies.
(() => {
  'use strict';
  const apply = () => {
    if (document.getElementById('literust-cosmetic-filter')) return;
    const style = document.createElement('style');
    style.id = 'literust-cosmetic-filter';
    style.textContent = `
      iframe[src*="doubleclick.net"],
      iframe[src*="googlesyndication.com"],
      ins.adsbygoogle,
      [data-ad-client][data-ad-slot],
      [id^="google_ads_iframe_"] {display:none !important}`;
    (document.head || document.documentElement)?.appendChild(style);
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', apply, { once: true });
  } else {
    apply();
  }
})();
