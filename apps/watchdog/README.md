# vitals-watchdog

Notices when the computer is lagging or an app has frozen, names the
process behind it, and offers to end it from a notification. It never ends
anything on its own. See [ADR-0032](../../docs/adr/0032-lag-watchdog.md).

```powershell
cargo build -p vitals-watchdog --release
.\target\release\vitals-watchdog.exe --install     # start at every logon, no admin
.\target\release\vitals-watchdog.exe --diagnose    # 10 s sample, prints the verdict, changes nothing
.\target\release\vitals-watchdog.exe --test-toast  # a real proposal whose buttons do nothing
.\target\release\vitals-watchdog.exe --uninstall
```

`--install` copies the exe to `%LOCALAPPDATA%\Vitals\watchdog\`, so a
rebuild or `cargo clean` cannot remove the program Windows starts at logon.
The event log (proposals, clicks, results) is `watchdog.log` in the same
folder.

It will not offer to end a shell, terminal, IDE or service host that has
children, anything in the session chain (`explorer`, `svchost`, `winlogon`,
…), a critical or protected process, or a process that already runs at
below-normal priority.

## When the desktop itself freezes

If `explorer.exe` (taskbar, Start) or `dwm.exe` (draws every window) stops
answering for five seconds, as the foreground window or as the taskbar, the
notification offers **Restart** instead of End:

- **Restart Explorer** ends the hung `explorer.exe`. Windows normally starts
  a new one within a second; if none appears within 5 s, the watchdog starts
  `%SystemRoot%\explorer.exe` itself. Open windows of other apps survive.
- **Restart the desktop** ends `dwm.exe` after a UAC prompt (it runs under
  another account), and Windows starts it again at once.

A hung shell may not be able to show a clickable notification, so the
same restarts are also available from the keyboard at any time:

| Keys             | Does                                 |
| ---------------- | ------------------------------------ |
| Ctrl+Alt+Shift+E | Restart Explorer                     |
| Ctrl+Alt+Shift+D | Restart the desktop (asks for admin) |

If another app already uses one of these combinations, it is skipped and
`watchdog.log` records a warning.

## Before memory runs out

Windows can only hand out as much memory as RAM plus the page file (the
_commit limit_). When programs have reserved 90 % or more of it for 4 of 6
seconds, the watchdog warns and names the process holding the most memory.
At 100 %, programs and `dwm.exe` start crashing even with RAM free. This
usually means WSL or Docker (`vmmemWSL`/`vmmem`), and the warning says so;
`wsl --shutdown` frees that memory. End is offered only for a process that
can safely be ended. `--diagnose` shows the commit percentage on every line
and names the largest holder.
