# 0003 — LAN security: off by default, scoped bearer tokens, plain HTTP

- Status: Accepted
- Date: 2026-09-10
- Tracker: D3

## Context

The LAN server (ADR 0001) can end processes. It runs on a home or office
network where other devices are not necessarily trusted. TLS on a private IP
needs a certificate the phone will not trust without ceremony, and the
ceremony is what stops people using the feature.

## Decision

- **Off by default.** Nothing binds a socket or advertises over mDNS until
  the user turns remote access on; both stop when it is turned off.
- **Bearer tokens**, generated with `getrandom`, shown once in full and as
  an 8-character prefix afterwards. Revocable individually.
- **Two scopes.** `Read` and `Control`; a new pairing is read-only. Read
  scope attempting `POST /api/v1/control` gets 403 and the controller is
  never called.
- **Constant-time comparison** against every stored token, so timing cannot
  reveal list position. A wrong token and a missing token produce
  byte-identical 401s.
- **The token travels in the URL fragment**, which browsers do not send to
  the server and which does not appear in access logs. The page strips it
  from the address bar after reading it.
- **Authorisation before validation.** `control` takes `Bytes` and parses by
  hand, so an unauthorised caller cannot probe the schema by watching 422
  versus 403.
- **Plain HTTP.** Accepted trade-off; documented in SECURITY.md.

## Consequences

- A passive attacker on the LAN can read frames in transit. The scope model
  limits what an active attacker can do with a captured read token to
  reading.
- Half the server's socket tests are attacks — anonymous request to every
  route, wrong token, read-scope control, path traversal — and stay that way.
- Secure-context-only browser APIs are unavailable to the phone (ADR 0029).
