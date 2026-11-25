import { describe, expect, it } from 'vitest';

import { makeCollabThread } from './fixtures.test-utils';
import { firstLine, newestFirst } from './collabModel';

describe('collab model', () => {
  it('collapses a multi-line ask into one row so the list cannot break its own layout', () => {
    expect(firstLine('first line\n\nsecond line\tthird')).toBe('first line second line third');
  });

  it('truncates a long message on a word boundary rather than mid-token', () => {
    const long = `alpha beta gamma ${'x'.repeat(40)}`;
    const out = firstLine(long, 20);
    expect(out.endsWith('…')).toBe(true);
    expect(out.length).toBeLessThanOrEqual(21);
    // The last kept word is whole: no split token before the ellipsis.
    expect(out.slice(0, -1).trimEnd().split(' ').at(-1)).toBe('gamma');
  });

  it('leaves a message that already fits untouched', () => {
    expect(firstLine('short and complete', 140)).toBe('short and complete');
  });

  it('sorts threads by newest activity and keeps an unknown timestamp last', () => {
    const older = makeCollabThread({ id: 'older', lastMs: 1_000 });
    const newer = makeCollabThread({ id: 'newer', lastMs: 2_000 });
    const unknown = makeCollabThread({ id: 'unknown', lastMs: null, askedAtMs: null });
    const ordered = newestFirst([older, unknown, newer]);
    expect(ordered.map((t) => t.id)).toEqual(['newer', 'older', 'unknown']);
  });

  it('does not mutate the list it is given', () => {
    const threads = [
      makeCollabThread({ id: 'a', lastMs: 1 }),
      makeCollabThread({ id: 'b', lastMs: 2 }),
    ];
    const ordered = newestFirst(threads);
    expect(threads.map((t) => t.id)).toEqual(['a', 'b']);
    expect(ordered).not.toBe(threads);
  });
});
