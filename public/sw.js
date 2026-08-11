const CACHE_NAME = "tapconductor-web-v2";
const APP_SHELL = ["./", "./index.html", "./manifest.webmanifest", "./app-icon.svg"];
const CACHEABLE_DESTINATIONS = new Set(["audio", "font", "image", "script", "style", "worker"]);

function isCacheableRequest(request) {
  if (request.method !== "GET" || request.headers.has("authorization")) return false;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return false;
  const scope = new URL(self.registration.scope);
  if (!url.pathname.startsWith(scope.pathname)) return false;
  const relativePath = url.pathname.slice(scope.pathname.length);
  return relativePath === ""
    || relativePath === "index.html"
    || relativePath === "manifest.webmanifest"
    || CACHEABLE_DESTINATIONS.has(request.destination)
    || /\.(?:wasm|musicxml)$/i.test(relativePath);
}

function isCacheableResponse(response) {
  const cacheControl = response.headers.get("cache-control") ?? "";
  const restricted = cacheControl
    .split(",")
    .some((directive) => /^(?:no-store|private)(?:\s|=|$)/i.test(directive.trim()));
  return response.ok
    && response.type !== "opaque"
    && !restricted;
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME)
      .then((cache) => cache.addAll(APP_SHELL))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(
        keys.filter((key) => key !== CACHE_NAME).map((key) => caches.delete(key)),
      ))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  if (!isCacheableRequest(event.request)) return;
  event.respondWith(
    caches.match(event.request).then((cached) => {
      const fetched = fetch(event.request)
        .then((response) => {
          if (isCacheableResponse(response)) {
            const copy = response.clone();
            void caches.open(CACHE_NAME).then((cache) => cache.put(event.request, copy));
          }
          return response;
        })
        .catch(() => cached ?? Response.error());
      return cached ?? fetched;
    }),
  );
});
