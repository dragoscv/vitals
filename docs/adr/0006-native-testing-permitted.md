# 0006 — Native runs are part of verification

- Status: Accepted
- Date: 2026-09-10
- Tracker: D6

## Context

An earlier constraint forbade launching the real app during agent sessions.
Under that constraint, two features (the tray icon and the HUD) were tested
only through mocked `invoke` calls, and the resubscribe defect that blanked
the dashboard was invisible to every unit test.

## Decision

The constraint is lifted. `tauri dev`, release builds and launching the app
are permitted, and the real WebView2 may be driven over CDP
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=…`).

## Consequences

- "Look at it" is a rule in `AGENTS.md`: a screen that renders is not a
  screen that is right.
- Verification-log entries record live CDP sessions alongside test counts.
- Compositor behaviours (transparent window, click-through, `skip_taskbar`)
  are still only provable by eye; the log says so when they were not checked.
