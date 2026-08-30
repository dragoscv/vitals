# Security Policy

## Why this matters here

Vitals installs an optional helper service that runs with SYSTEM privileges and
accepts commands over a local named pipe. It can terminate processes, write
firewall rules and change power policy. A flaw in that surface is a local
privilege-escalation vulnerability, not a cosmetic bug.

The design deliberately keeps that surface small:

- The UI itself never runs elevated. The webview — the largest attack surface —
  has no privileges beyond the user's own.
- The pipe's DACL restricts access to the installing user and administrators.
- The helper verifies the client's image path and signature before accepting
  any command.
- Every command is an enum variant, never a shell string. There is no parser to
  escape and no injection surface.
- Mutating commands are logged with the caller's SID.

## Reporting a vulnerability

**Please do not open a public issue.**

Use GitHub's [private vulnerability reporting](https://github.com/vitals-app/vitals/security/advisories/new),
which is enabled on this repository.

Please include:

- What the flaw allows an attacker to do.
- Steps to reproduce, or a proof of concept.
- The affected version and your OS build.

You will get an acknowledgement within 72 hours and an assessment within seven
days. If the report is valid we will agree a disclosure timeline with you, and
credit you in the advisory unless you prefer otherwise.

## Scope

**In scope**

- Anything that lets an unprivileged process drive the helper service.
- Escaping the webview's sandbox or capability restrictions.
- Code execution via a crafted plugin manifest, update manifest or report file.
- The updater: signature bypass, downgrade attacks, or MITM.

**Out of scope**

- Requiring administrator rights to begin with. An administrator can already do
  everything the helper does.
- Anything requiring physical access to an unlocked machine.
- Third-party plugins not distributed by this project.
- Denial of service caused by deliberately exhausting your own machine's
  resources.

## Supported versions

During pre-1.0 development only the latest release receives fixes.
