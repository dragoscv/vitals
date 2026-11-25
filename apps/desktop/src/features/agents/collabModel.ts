/**
 * Pure logic for the collaboration page: the formatting decisions that do not
 * need React, kept apart so they are unit-tested without rendering.
 *
 * The threading itself happens in Rust (`crates/vitals-agents/src/collab.rs`)
 * against agentq's own files, so this file holds only what a row needs.
 */

import type { CollabThread } from '@vitals/protocol';

/** How many characters of a message a thread row shows before truncating. */
const SUMMARY_MAX = 140;

/**
 * The first line of a message, for a one-line summary.
 *
 * A message is prose an agent wrote — often several paragraphs with numbers
 * and question marks — and a list row shows its opening. Every run of
 * whitespace collapses to a single space so a multi-line ask cannot break the
 * row, and a long message is cut on a word boundary rather than mid-token.
 */
export function firstLine(message: string, max = SUMMARY_MAX): string {
  const flat = message.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  const cut = flat.slice(0, max);
  const space = cut.lastIndexOf(' ');
  return `${space > 0 ? cut.slice(0, space) : cut}…`;
}

/**
 * The threads, newest activity first.
 *
 * The backend already sorts them; this reapplies the same rule where a test
 * can see it, and it is deliberately total — a thread with no timestamp at
 * all sorts last rather than jumping to the top of a list about what is recent.
 */
export function newestFirst(threads: readonly CollabThread[]): readonly CollabThread[] {
  return [...threads].sort((a, b) => (b.lastMs ?? 0) - (a.lastMs ?? 0));
}
