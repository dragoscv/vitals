# Architecture decision records

One file per decision, in [MADR](https://adr.github.io/madr/) form: status,
context, decision, consequences. A record is never edited once accepted; a
change of mind is a new record that supersedes the old one.

D-numbers refer to the decision table in [`../TRACKER.md`](../TRACKER.md).

| ADR                                                    | Decision                                                | Tracker |
| ------------------------------------------------------ | ------------------------------------------------------- | ------- |
| [0001](0001-embedded-lan-server-and-mobile-pwa.md)     | Phone access via an embedded LAN HTTP server and a PWA  | D1      |
| [0002](0002-third-party-api-surface.md)                | REST + SSE + WebSocket + Prometheus + loopback + SDK    | D2      |
| [0003](0003-lan-security-model.md)                     | LAN security: off by default, scoped bearer tokens      | D3      |
| [0004](0004-cli-samples-directly.md)                   | The CLI samples directly when the app is closed         | D4      |
| [0005](0005-upgrade-to-latest-stable-in-slices.md)     | Upgrade everything to latest stable, in gated slices    | D5      |
| [0006](0006-native-testing-permitted.md)               | Native `tauri dev`/build runs are part of verification  | D6      |
| [0007](0007-windows-only-honest-stubs.md)              | Windows only; macOS and Linux stay honest trait stubs   | D7      |
| [0008](0008-accept-the-nine-nice-to-haves.md)          | Accept all nine "nice-to-have" features                 | D8      |
| [0009](0009-tracker-csv-plus-markdown.md)              | Tracker as CSV (state) plus Markdown (reasoning)        | D9      |
| [0010](0010-conventional-commits-per-slice-no-push.md) | Conventional Commits per slice, explicit paths, no push | D10     |
| [0011](0011-dead-crates.md)                            | Implement `vitals-store`, delete `vitals-plugin`        | D11     |
| [0012](0012-helper-stays-stubbed.md)                   | `apps/helper` stays stubbed; disk I/O from DiskCounters | D12     |
| [0013](0013-inert-settings.md)                         | Wire or delete every inert setting                      | D13     |
| [0014](0014-separate-mobile-entry.md)                  | A separate lean `mobile.html` entry                     | D14     |
| [0015](0015-mobile-scope.md)                           | Mobile scope includes process control and multi-machine | D15     |
| [0016](0016-palette-on-radix-dialog.md)                | Command palette on Radix Dialog; remove `cmdk`          | D16     |
| [0017](0017-motion-lazy-domanimation.md)               | `motion/react-m` under `LazyMotion(domAnimation)`       | D17     |
| [0018](0018-size-budget-raised-deliberately.md)        | Size budget raised deliberately, gate stays strict      | D18     |
| [0019](0019-git-remote-never-push.md)                  | `origin` → `dragoscv/vitals`, never push from an agent  | D19     |
| [0020](0020-updater-keys.md)                           | Updater fully wired; key generation is the user's step  | D20     |
| [0021](0021-slice-order.md)                            | Slice order: tooling → truth → server → … → docs/CI     | D21     |
| [0022](0022-accept-the-eleven-extras.md)               | Accept all eleven extras                                | D22     |
| [0023](0023-resolver-3-and-edition-2024.md)            | Cargo resolver 3 and Rust edition 2024                  | S10-02  |
| [0024](0024-dual-typescript-compilers.md)              | TypeScript 7 for `tsc`, TypeScript 6 for ESLint         | S10-02  |
| [0025](0025-nsis-per-machine.md)                       | NSIS installer, `perMachine`                            | S10-02  |
| [0026](0026-panic-abort-in-release.md)                 | `panic = "abort"` in release                            | S10-02  |
| [0027](0027-ts-rs-over-specta.md)                      | `ts-rs` over `specta` for generated bindings            | S10-02  |
| [0028](0028-axum-for-the-lan-server.md)                | axum for the LAN server                                 | S10-02  |
| [0029](0029-no-service-worker.md)                      | No service worker in the mobile app                     | S10-02  |

## Writing a new one

Copy the shape of any file here. Number it next in sequence, name it
`NNNN-slug.md`, add a row above, and reference it from the commit that
implements it. British spelling.
