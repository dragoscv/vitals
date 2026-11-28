# 0052 — The Tracking page claims a task, and writes only the task store

- **Status:** Accepted (2026-11-28)
- **Slice:** S26-37
- **Supersedes:** none

## Context

ADR 0051 put agentq's tracker index on the Tracking page and made it read-only,
because a tracker is the record of a repository and Vitals is not a second
writer of it. That decision stands and this one does not touch it: no command
here writes a tracker file and the page still offers no action on a tracker row.

What the page could not do was the thing a resume needs. A session's claims —
"this chat is carrying S26-37, and its subagent is carrying S26-38" — live in
agentq's task store, `<coord>/tasks/<id>.json`, written by `track.mjs`. Seeing
them is useful; taking one is the difference between a dashboard and a place to
work from. The owner asked for the page to claim and complete a task, so that
another machine or window can pick the work up from the page alone.

`track.mjs` is a Node CLI in the agentq clone. It has no installed shim (unlike
`agentq.ps1`, which is at `~/.copilot/bin`), and nothing in the environment or
in any config file names its path, so from inside a Tauri app there is no
reliable way to locate it. Three routes were available: find the tool and shell
out to it; write the task store from Rust; or drop the action.

## Decision

Vitals writes the task store itself, in Rust, replicating `track.mjs`'s
protocol rather than approximating it.

1. `crates/vitals-agents/src/tasks.rs` holds the reads and the one write. It
   resolves the coordination directory through the **same**
   `vitals_agents::agentq::default_root()` the index read uses — one resolution
   point for `AGENTQ_HOME`, `USERPROFILE` and `HOME`, not a second one.
2. The write is narrow by construction: `claim` and `complete`, nothing else.
   Nothing here creates a task, deletes one, or changes its id, title, group or
   dependencies. An id that could escape `tasks/` (`..`, a separator, a control
   character, or a path longer than 200 characters) is refused rather than
   normalised.
3. The protocol is copied exactly, because a second writer that drifts is worse
   than none: the same `<file>.lock` taken with create-or-fail, the same 3 s
   bounded wait before taking a stale lock over, the same temp-then-rename
   atomic write, the same `CLAIMABLE` set (`todo`, `claimed`, `in-progress`), the
   same rule that an owner other than the caller is refused, and the same
   timestamps in the same shape (`new Date().toISOString()`). `track.test.mjs`
   and this crate's tests pin the two sides of that contract.
4. Two Tauri commands, both `async` over `spawn_blocking` because they touch the
   disk: `get_resume_pack` reads the current session, its plan and every task,
   and `claim_tracker_task` performs one action. The write returns
   `Result<(), String>`; a refusal is the store's own words, shown to the person
   who clicked.
5. The claims are rendered only where the store would accept them. The panel
   draws a **Complete** button on a task this session holds and a **Claim**
   button on an open task nobody holds; a task held by a subagent gets no button
   at all, because `track.mjs` would refuse the claim and a control that cannot
   succeed is a lie. Where there is no session to claim as, the button is
   disabled and the reason is written beside it, not hidden in a tooltip.
6. The page re-reads after a write rather than assuming it succeeded: the only
   evidence a claim landed is the store saying so.

## Consequences

- The page can hand work to a session, and a resume from another machine can
  finish it. The cost is a second writer of a format owned by `track.mjs`; it is
  mitigated by copying the protocol exactly and by the fact that the format is
  three fields of JSON, but the mitigation is a contract, not a proof — if
  `track.mjs` changes its lock protocol, this crate must change with it.
- Least privilege holds in the store's own terms: a claim moves a task between
  two of its existing statuses and a completion adds evidence. Neither can
  destroy a record, and neither touches a tracker (ADR-0051).
- Rejected: locating and spawning `track.mjs` (a Node runtime as a dependency of
  a Rust monitor, and no pointer to it exists); a `track` verb on `agentq.ps1`
  (that script has no such verb, and adding one is a change to another
  repository's tool for one caller); caching the pack on disk (a rebuilt view of
  live state, with a staleness question nothing needs to answer — one read per
  visit and a re-read after each write is enough).
