// Service worker: bust all caches on every fetch
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then(names => Promise.all(names.map(n => caches.delete(n))))
      .then(() => self.clients.claim())
  );
});
self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  // Only bust cache for our own files, not CDN imports
  if (url.origin === self.location.origin && !url.pathname.startsWith('/api') && !url.pathname.startsWith('/ws')) {
    url.searchParams.set('_cb', Date.now());
    e.respondWith(fetch(url.toString(), { cache: 'no-store' }));
  }
});
