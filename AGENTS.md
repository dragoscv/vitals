# Working in this repository

Vitals is a Windows system monitor: a Tauri 2 desktop app (React 19 webview,
Rust backend), a set of Rust crates, a LAN HTTP server, and a CLI. Read this
before changing anything. Every rule below exists because its absence caused a
real defect — the commit is cited where it helps.

## The one principle

**A number that cannot be measured is `None`, never `0`.** An unmeasured
reading and a genuinely-zero reading are different facts, and conflating them
turns "this GPU does not report VRAM" into "this GPU is using 0 MB". Every
optional metric in `vitals-core` is `Option<T>` for this reason, the
Prometheus exporter omits absent series rather than emitting zero, and the UI
renders an em dash. Do not add a `.unwrap_or(0)` to make a type simpler.

## Verify, do not assume

Nothing is "done" because it compiles.

- Run the verifying command and read its output before saying it works.
- `pwsh -NoProfile -File scripts/verify.ps1` runs every gate CI runs.
- **Never put `git commit` in the same shell command as the gates.** The
  commit runs regardless of the earlier exit codes; this shipped two type
  errors and cost a fix-up commit (`217e5bc`).
- A green test is not evidence until you know what would make it fail. If you
  cannot answer that, mutate the code and watch it go red. Two tests in this
  repo were decorative until someone tried.

### The prover pattern

Each subsystem has a `cargo run -p <crate> --example prove_*` that exercises it
against the real machine and prints what it found. Fixtures cannot show that a
sampler's actual output survives serialisation, storage and a client's parser.
Both defects found on 2026-09-10 came from a prover, not from a test:

- `prove_store` — writes real frames to SQLite, reads them back, prints sizes.
- `prove_lan` — samples, serves over a socket, prints what a phone receives.

Add one for any subsystem with an external boundary. Run the existing ones
after touching the code they cover.

### Look at it

Two of the four defects found on 2026-09-10 were invisible to every test and
obvious within ten seconds of opening the page: `PID 0 · 43 %` at the top of
the process list, and `GPU 0 %` beside a real GPU at 16 %. Both were
backend bugs; the phone was simply the first client to render the data
without the desktop's incidental filtering.

`cargo run -p vitals-server --example serve_dev` serves live metrics on
:7332 with the token `dev`, so any client can be pointed at a real machine.
Open the thing you built and read the numbers. A screen that renders is not
a screen that is right.

### A new client is an audit

Every time this project grows a consumer — the LAN API, Prometheus, the
SDK, the phone — it finds bugs in code that was already "done". The reason
is always the same: the existing consumer happened to hide the problem. When
you add one, expect to fix the backend, and fix it **at the source** so
every consumer benefits rather than patching the new client.

## Contracts that compile separately

Two sides that build independently will drift, and no compiler will say so.
`pwsh -NoProfile -File scripts/check-drift.ps1` checks all of these in about
two seconds; it runs in CI and in the pre-commit hook.

1. **serde casing vs `ts_rs`.** Any struct deriving `ts_rs::TS` must carry
   `#[serde(rename_all = "camelCase")]`. The TypeScript binding is camelCase
   unconditionally; serde defaults to the Rust field name. `ProcessKey`
   shipped `start-time` against a binding that said `startTime`, and the
   guard in `wire_format.rs` missed it because it only looked for `_`.
2. **`invoke('name')` vs `#[tauri::command]`.** A typo is a runtime failure in
   the webview and nothing else. Commands must be in `generate_handler!` to be
   reachable — `set_process_priority` existed for weeks, registered nowhere,
   while the UI reported the feature as unimplemented.
3. **Locale parity.** Every key in one locale exists in the other. CLDR plural
   suffixes (`_few`, `_many`) are exempt: Romanian needs forms English does not.
4. **Generated bindings.** `cargo test -p vitals-core --features ts` regenerates
   `packages/protocol/src/generated`; a dirty tree afterwards means drift.

Two more that no script can check, so they are on you:

5. **A template or query string that addresses a field by name.** The Home
   Assistant package's Jinja paths and the Prometheus dashboards read fields
   by dotted name; rename one in Rust and they silently read nothing forever.
   `crates/vitals-server/tests/home_assistant.rs` resolves every path against
   a real serialised frame — extend it when you add a sensor.
6. **Documentation that states behaviour.** `docs/api/openapi.yaml` has a
   drift test for its route list, but not for its prose. If you change a
   status code or an ordering, grep the docs.

## Authorisation before validation

Check who is asking before checking what they asked. An extractor that
parses the body runs _before_ the handler, so `Json<T>` as an argument means
a malformed request from an unauthorised caller gets 422 rather than 403 —
which lets them probe the schema by watching the status change. Take
`Bytes` and parse by hand where the order matters (`control` in `router.rs`).

## Tests

- A test's name is a full sentence stating the guarantee:
  `fn exits_apply_before_changes_so_a_recycled_pid_survives()`.
- Decorative tests are banned. Prefer one adversarial test to five happy paths.
  In `vitals-server`, half the socket tests are attacks: anonymous request to
  every route, wrong token, read-scope token attempting control, path traversal.
- Test the **guarantee**, not the implementation. "A new client receives a
  keyframe" passed while still serving thirty-second-old data; the real
  assertion was that the delta had been applied.
- Shared fixtures live behind `vitals-core`'s `fixtures` feature. Use
  `fixtures::system()` / `::process()` / `::keyframe()` rather than
  hand-rolling a thirty-field struct per crate — four drifting copies of a
  "plausible machine" is how a field addition breaks four files.

## Rust

- `cargo clippy --workspace --all-targets -- -D warnings` must pass. The lint
  set is pedantic: `expect_used` and `unwrap_used` are denied outside tests,
  along with `too_many_lines`, `doc_markdown`, `match_same_arms`,
  `result_large_err` and `format_push_string`.
- An `#[allow(...)]` needs a comment saying why. "Clippy complained" is not why.
- Public fallible functions document `# Errors`.
- Platform code lives in `vitals-win` / `vitals-linux` / `vitals-macos` behind
  the traits in `vitals-core`. If `vitals-core` stops compiling on Linux, the
  trait boundary has leaked — CI builds it on Ubuntu to catch exactly that.

## TypeScript

- `exactOptionalPropertyTypes` is **on**. This is the trap that catches
  everyone: you cannot pass `undefined` to an optional property. Use
  `...(x !== undefined && { key: x })`, not `key: x ?? undefined`.
- `noUncheckedIndexedAccess` is on: `array[0]` is `T | undefined`.
- `verbatimModuleSyntax` is on: `import type` for type-only imports.
- ESLint is type-aware. No floating promises, no `any`, throw real `Error`s.
- Types cross the boundary from `@vitals/protocol`, which is generated from
  Rust. Never hand-write a type that already exists there.

## User-facing text

- Every string goes through i18n, in **both** `en` and `ro`. App strings live
  in `packages/i18n/src/locales`; shell strings in
  `apps/desktop/src/shell/strings.ts`.
- Write for someone who does not know what a working set is. "Memory in use",
  not "WS Private". The audience is a person whose computer is slow.
- British spelling in prose and in comments.

## Comments

Explain **why**, never what. A comment that restates the code is noise; a
comment that records the reason a decision was made is the most valuable line
in the file. Especially: why a simpler approach was rejected, what breaks if
this is changed back, and which real failure a guard exists for.

## Security

- Remote access is **off by default** and nothing binds a socket or broadcasts
  until the user turns it on. mDNS advertisement starts with the server and is
  torn down with it.
- Tokens are compared in constant time, against every token, so timing cannot
  reveal list position. A wrong token and a missing token return byte-identical
  401s — distinguishing them is a free oracle.
- A pairing secret is shown once. Everything afterwards displays an 8-character
  prefix. Never log a token, never put one in a request line (the QR carries it
  in the URL fragment, which is not sent to the server).
- Read scope cannot control. The default for a new pairing is read-only.

## Performance

- The sampler runs continuously on someone's machine. Work that is only needed
  when a feature is on must not happen when it is off — the LAN server's frame
  clone is gated on `is_running()` for this reason.
- `cargo test -p vitals-win --release --test overhead` is a budget, not a
  benchmark. It fails the build.
- Measure before optimising, and again after. If a change produced no
  measurable improvement, say so and revert it.

## Before you finish

- `pwsh -NoProfile -File scripts/verify.ps1`
- `pwsh -NoProfile -File scripts/hooks/install.ps1` once per clone; the
  pre-commit hook catches formatting, drift, secrets and `.only` in seconds.
- Update `docs/tracker.csv` (state) and `docs/TRACKER.md` (reasoning, plus a
  verification-log entry with real command output).
- Close the ripple: callers, both locales, the CLI, the SDK, the docs. Say
  explicitly what you did **not** update and why.
