# 0020 — Wire the updater completely; key generation is the owner's one-off step

- Status: Accepted
- Date: 2026-09-10
- Tracker: D20

## Context

The updater `pubkey` was the literal string
`REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY` and nothing called `check()` (F6).
An agent must not generate the signing key pair: the private key would then
exist in a session log.

## Decision

Everything around the key is wired: the plugin, the `check()` call, an
honest UI state (checking / up to date / available / error), the release
workflow writing `latest.json` with the minisign signature, and a build that
refuses to publish a manifest when the signature is missing. The owner runs
`tauri signer generate` once and commits the public key; the private key
and password go into GitHub secrets. That step has since been done: the
committed `pubkey` is a real minisign key.

## Consequences

- An unsigned update is one the app refuses to install; a compromised mirror
  cannot push a payload.
- Nightlies do not write a manifest — their tag moves, and an updater pointed
  at a moving target would offer the same "new" version forever.
- Rotating the key is a new decision: every installed app trusts the current
  one.
