# 0029 — No service worker in the mobile app

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

A PWA conventionally ships a service worker for offline caching and
install prompts. The phone app is served over `http://192.168.x.x`
(ADR 0003), which browsers do not treat as a secure context:
`navigator.serviceWorker` is `undefined` and `beforeinstallprompt` never
fires. `vite-plugin-pwa` would generate a worker that never registers.

## Decision

No service worker and no `vite-plugin-pwa`. `mobile.html` carries an inline
`data:` manifest and Add-to-Home-Screen meta tags, which is enough for
"Add to Home Screen" on both mobile platforms.

## Consequences

- No offline mode; the data is live at 1 Hz and stale numbers would be
  worse than an error.
- The manifest is inline so the page stays a single file to serve.
- If the server ever gains TLS, this decision should be revisited — a worker
  would then be able to cache the shell.
