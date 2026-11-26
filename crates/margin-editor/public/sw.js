// Offline copy of the notebook for the installed web app. Only the app's own
// files are cached; notes never pass through here.
const CACHE = "margin-shell-v2";
const SHELL = ["./", "manifest.webmanifest", "icon.svg", "icons/apple-touch-icon.png", "icons/icon-192.png", "icons/icon-512.png"];
// The bundler names the script, the wasm and the fonts after a hash of their
// content: a file with such a name never changes, and a new build has new names.
const HASHED = /^(.*)-dxh[0-9a-f]+(\.\w+)$/;

self.addEventListener("install", event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", event => {
  event.waitUntil(caches.keys()
    .then(keys => Promise.all(keys.filter(key => key !== CACHE).map(key => caches.delete(key))))
    .then(() => self.clients.claim()));
});

// Keep a copy, and drop the copies an earlier build left under the same name.
async function keep(cache, request, response) {
  await cache.put(request, response);
  const [, stem, extension] = new URL(request.url).pathname.match(HASHED) ?? [];
  if (!stem) return;
  for (const other of await cache.keys()) {
    const [, otherStem, otherExtension] = new URL(other.url).pathname.match(HASHED) ?? [];
    if (otherStem === stem && otherExtension === extension && other.url !== request.url) await cache.delete(other);
  }
}

// A page loads its script, wasm and fonts before this worker controls it, so
// the first visit lists what it loaded: one visit is then enough to open offline.
self.addEventListener("message", event => {
  if (event.data?.type !== "loaded") return;
  event.waitUntil(caches.open(CACHE).then(cache => Promise.all(event.data.urls
    .filter(url => url.startsWith(registration.scope))
    .map(async url => {
      if (await cache.match(url)) return;
      const response = await fetch(url).catch(() => null);
      if (response?.ok) await keep(cache, new Request(url), response);
    }))));
});

self.addEventListener("fetch", event => {
  const { request } = event;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== location.origin) return;
  event.respondWith(caches.open(CACHE).then(async cache => {
    const saved = await cache.match(request, { ignoreSearch: true });
    // A hashed file is the same forever: no need to ask the network again.
    if (saved && HASHED.test(url.pathname)) return saved;
    // Everything else comes from the network, so a new deploy shows on the
    // next launch; the saved copy answers only when offline.
    try {
      const response = await fetch(request);
      if (response.ok) event.waitUntil(keep(cache, request, response.clone()));
      return response;
    } catch {
      return saved ?? (request.mode === "navigate" ? await cache.match("./") : null) ?? Response.error();
    }
  }));
});
