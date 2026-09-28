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
