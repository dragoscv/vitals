# Getting help

Vitals is maintained by one volunteer. Help is given on a best-effort basis,
with no guaranteed response time. The fastest answers come from a clear
report in the right place.

## Where to go

| You have                                    | Go to                                                                                                                                       |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| A question, an idea, or "is this expected?" | [GitHub Discussions](https://github.com/dragoscv/vitals/discussions)                                                                        |
| A bug, a crash or a number that looks wrong | [Open an issue](https://github.com/dragoscv/vitals/issues/new/choose) using one of the forms                                                |
| A security vulnerability                    | [Report it privately](https://github.com/dragoscv/vitals/security/advisories/new), never as a public issue. See [SECURITY.md](SECURITY.md). |
| A question about privacy or your data       | [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com). See [PRIVACY.md](PRIVACY.md).                                                          |

Documentation lives at [vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro).

## What to include in a bug report

- **System details.** In Vitals, open Settings → About → **Copy system
  details** and paste the result. It contains the app version, Windows build
  and hardware summary.
- **What you did, what you expected, what happened.** Steps that reproduce
  the problem are worth more than anything else.
- **For a wrong number:** what Task Manager, HWiNFO or your BIOS shows for
  the same reading, and your exact hardware model.
- **Logs**, if the app misbehaved or crashed. They are in
  `%LOCALAPPDATA%\Vitals\logs`:
  - `vitals.log` — the diagnostic log.
  - `crash.txt` — details of the last crash, if there was one.

Read logs before attaching them. They contain process names and file paths
from your computer; remove anything you do not want to be public.

## What to expect

- Issues are triaged when the maintainer has time, usually within a couple of
  weeks. Security reports are acknowledged within 72 hours.
- A report that can be reproduced is usually fixed first.
- Vitals is a public beta (0.9.x). Only the latest release receives fixes;
  please update before reporting.
- macOS and Linux builds are not published yet, so reports for those systems
  cannot be acted on.
