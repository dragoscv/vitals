---
name: add-a-command
description: Add or change a Tauri command in Vitals — the IPC boundary between the React webview and the Rust backend, including registration, error mapping, the LAN equivalent, and capability reporting. Use when the UI needs the backend to do something, or when an invoke() call fails at runtime.
---

# Adding a Tauri command

The whole risk here is that this boundary has no compiler. `invoke('typo')` is
a runtime failure in the webview and nothing else; a command that is not in
`generate_handler!` is unreachable no matter how correct it is. Both have
happened in this repo — `set_process_priority` was implemented, advertised in
the capability report, and registered nowhere, while the UI called the feature
unimplemented.

## 1. Write it — `apps/desktop/src-tauri/src/commands.rs`

```rust
/// One sentence on what it does for the user.
///
/// Any non-obvious constraint goes here, especially why an argument exists.
#[tauri::command]
#[cfg(windows)]
pub fn do_the_thing(pid: u32, start_time: u64) -> CommandResult<()> { … }
```

- **Anything acting on a process takes `pid` AND `start_time`.** Windows
  reuses PIDs aggressively; between the frame that listed a process and the
  click that acts on it, the PID can belong to something else. `ProcessKey`
  exists to close that race and every mutating provider API demands it.
- Arguments arrive **camelCase** from the webview and are matched to snake_case
  Rust parameters by Tauri. Return types are serialised by your serde
  attributes — a DTO returned to the UI needs `rename_all = "camelCase"`.
- Return `CommandResult<T>`. The error type maps `AccessDenied` to an
  elevation affordance rather than a failure toast, so do not flatten errors
  into strings.
- Long work does not belong on the IPC thread. If it can take more than a few
  milliseconds, make it `async` or hand it to the sampler thread.

## 2. Register it — `apps/desktop/src-tauri/src/lib.rs`

Add it to `generate_handler![…]`. **This is the step that gets missed.** A
`#[cfg(windows)]` command needs the attribute in the list too.

## 3. Call it — the webview

Never `invoke` from a component. Put it in the feature's api/actions module
(`features/<x>/actions.ts`, `settings/lan/api.ts`) so it is mockable and there
is exactly one place the command name is spelled.

Take the API as an injectable interface where the screen is worth testing —
`RemoteAccessPanel` takes a `LanApi`, which is why it has six tests and no
Tauri host.

## 4. Mirror it on the LAN, or decide not to

If the action is something a paired phone should be able to do, add it to
`ControlRequest` in `crates/vitals-server/src/control.rs` and handle it in
`DesktopController::apply` in `apps/desktop/src-tauri/src/server.rs` — so the
phone and the context menu go through the same function with the same risk
checks. Then extend `packages/client`'s `ControlRequest`.

If it should stay desktop-only, say so in a comment. Silence reads as an
oversight.

## 5. Capabilities

If the UI shows or hides an affordance based on `get_capabilities`, the
capability must report `Available` only when the command is actually reachable.
A capability that claims more than the handler list provides is the exact bug
described above.

## 6. Verify

```
pwsh -NoProfile -File scripts/check-drift.ps1   # invoke() ↔ generate_handler!
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck; npx eslint .
```

`check-drift.ps1` fails on an `invoke` with no command, and **warns** on a
command nothing invokes. Read the warnings: every one so far has been a real
feature nobody could reach.

Then run the app and click the thing. An IPC boundary is not proven by a unit
test on either side of it.
