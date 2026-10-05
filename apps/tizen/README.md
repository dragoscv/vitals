# Vitals for Samsung TVs (Tizen)

A Tizen web app (`.wgt`) that watches the Windows PCs on your home network
from a Samsung TV, and shows what the TV itself will tell a web app about its
own load, memory, storage, network and screen. Same scope and wording as the
Google TV app in `apps/android/tv`.

Built for and verified on a Samsung Odyssey G8 (LS34DG850SU, Tizen 9.0,
Chromium M120, armv7, 3440×1440). `required_version` is 6.0.

## What it does

- **Overview** — this TV and every paired PC as focusable cards: CPU, memory,
  graphics and CPU heat rings, and a one-minute CPU sparkline.
- **This TV** — only what `tizen.systeminfo` offers: CPU load, memory total
  and available, storage units, network type, IP, Wi-Fi signal and SSID,
  display resolution, model, manufacturer, software build and the Tizen
  platform version. Anything the firmware does not report is an em dash.
  Temperatures and per-app figures are not available to a web app, and the
  screen says so.
- **PCs** — list and detail, with tabs:
  - **Now**: CPU with per-core bars, memory, each GPU, disks, networks, fans,
    battery and power draw (panels for hardware the PC does not have are
    left out, not filled with dashes).
  - **Programs**: sort by CPU, memory or name; the top 150; a panel with
    details and End task (two presses within 4 s), Pause/Resume, Priority
    (not real-time, which the PC refuses from a remote) and Efficiency mode.
    The actions disappear for a read-only pairing, learnt from the PC's
    first `403 forbidden` or from the pairing reply.
  - **Sensors**: `GET /api/v1/sensors` every 5 s while the tab is open.
  - **History**: 1 h / 24 h / 7 d from `GET /api/v1/history?seconds=N`,
    averaged into at most 240 buckets; a bucket with no sample is a gap,
    not a dip to zero.
  - **About**: `GET /api/v1/host` and the PC's Vitals version.
- **Add a PC** — the address (port 7331 by default), then the six-digit code
  from the PC's _Pair a TV_, typed on the remote's number keys or an
  on-screen keypad (`POST /api/v1/pair`). A token can be typed instead for a
  PC whose Vitals predates pairing codes. Only private addresses are
  accepted: 10/8, 172.16/12, 192.168/16, 169.254/16, 127/8, 100.64/10,
  `.local`, and `fc`/`fd`/`fe80` IPv6.
- **Settings** — paired PCs (remove is two presses), language
  (English / Română / same as the TV), version, privacy and source links.

Navigation is D-pad everywhere: arrows move by the spatial rule in
`src/lib/spatial.ts`, Enter activates, Return goes back and leaves the app at
the top level. Focus is a white 3 px ring plus a slight scale, never colour
alone. Margins are 5 % for overscan, and the root font size is a fraction of
the viewport height so 1080p and 1440p ultrawide show the same number of
lines, the extra width becoming columns.

### What it cannot do, and why

- **Wake-on-LAN**: needs a UDP broadcast; a Tizen web app has no UDP. The PC
  screen says this instead of offering a button that cannot work.
- **Find PCs on the network**: mDNS is UDP multicast too, so the address is
  typed. The PC shows it on its Remote access page.
- **Temperatures of the TV**: `tizen.systeminfo` has none.

## Talking to the PC without CORS

The PC's LAN server sends no CORS headers. That is fine here: a packaged
`.wgt` runs from `file://`, and with `<access origin="*" subdomains="true"/>`
and the `internet` privilege, Tizen lets it make cross-origin requests with
an `Authorization` header and read the reply. Proven on the Odyssey G8 on
2026-09-29 with a probe package: a readable 401 from an authenticated
`fetch`, a readable 400 from a JSON `POST`, and later a working stream with
a real token. It would **not** work from an ordinary web page.

Live frames use `VitalsClient.stream` from `@vitals/client` — server-sent
events with the token in the query string, the documented browser fallback,
because `EventSource` cannot send headers. The token is proven with
`/health` and `/snapshot` before the stream opens, since a browser cannot
see a 401 on an event stream.

`@vitals/client` has no `pair()`, `sensors()` or `history()` yet, so those
three calls are small local functions (`src/lib/pairing.ts`, `src/lib/api.ts`).

## Where the pairings are kept

In `localStorage`, which on the TV is the app's own sandbox, removed with
the app. A web app has no Keystore to encrypt them with, so each token is
stored in plain text. A token is a bearer credential for one PC, usable only
from the LAN (the PC's server listens on the local network, and this app
refuses to send one to a public address, even from a tampered store). If
the TV is lost or given away, remove the pairing on the PC (Settings →
Remote access) to revoke it.

## Build

```powershell
pnpm --filter @vitals/tizen build        # tsc + vite → apps/tizen/dist
pnpm --filter @vitals/tizen test
pnpm --filter @vitals/tizen dev          # desktop browser at :5283, no tizen API
```

The build is one classic (non-module) script with relative paths, targeting
Chromium 120: a module script or a `crossorigin` attribute is refused from
an opaque `file://` origin, and a lazily-loaded chunk would be fetched from
`file://` at runtime. `public/config.xml` and `public/icon.png` are copied
into `dist`. The icon is generated with every other brand asset by
`node brand/scripts/build.mjs`.

## Install on a TV

Needs Tizen Studio (`%USERPROFILE%\tizen-studio`, CLI and `sdb`) and a Samsung
certificate profile, and the TV in developer mode with this PC's IP set as
the developer host.

```powershell
pwsh -NoProfile -File apps/tizen/scripts/tv-deploy.ps1            # package, install, launch
pwsh -NoProfile -File apps/tizen/scripts/tv-deploy.ps1 -DevTools  # launch in debug, DevTools on :9222
node apps/tizen/scripts/cdp.mjs keys down,enter shot page.png focus
```

`tv-deploy.ps1` packages from a TEMP copy of `dist`, because `tizen package`
signs in place. `cdp.mjs` presses remote keys through
`Input.dispatchKeyEvent`, reads the page and saves screenshots.

**Certificate caveat.** The `mixai-samsung` profile's distributor
certificate lists one DUID, `RLCAX3YHYQ2UW` (the Odyssey G8 above). It will
not install on any other TV. Publishing to other people needs Samsung
Seller Office review and a store-signed package — not done.

## Tests

`vitest` over the pure parts: formatting (em dash for unmeasured, binary
units, locale decimals), the private-address rule, code cleaning, the
mapping of `/pair` answers (403 → bad code, 404 → older PC), history
bucketing with gaps, frame folding, process sorting and safety, control
failure messages, the spatial focus rule, and English/Romanian key and
placeholder parity (the repo's drift script does not cover this app).
