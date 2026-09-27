import { describe, expect, it } from 'vitest';

import { errorMessage, isCommandError } from './commandError';

describe('errorMessage', () => {
  it('reads the message out of a command error instead of printing [object Object]', () => {
    const rejected = { kind: 'access-denied', message: 'open process 6440 requires elevation' };
    expect(errorMessage(rejected)).toBe('open process 6440 requires elevation');
    // The exact regression: what every non-Processes screen used to show.
    expect(errorMessage(rejected)).not.toContain('[object Object]');
  });

  it('passes Tauri argument errors, which arrive as bare strings, through unchanged', () => {
    expect(errorMessage('invalid args `priority`')).toBe('invalid args `priority`');
  });

  it('reads a thrown Error', () => {
    expect(errorMessage(new Error('boom'))).toBe('boom');
  });

  it('shows the content of an unknown object rather than its type tag', () => {
    expect(errorMessage({ code: 5 })).toBe('{"code":5}');
  });

  it('recognises every kind the backend sends, including refused', () => {
    for (const kind of ['access-denied', 'not-found', 'unsupported', 'refused', 'internal']) {
      expect(isCommandError({ kind, message: 'm' })).toBe(true);
    }
    expect(isCommandError({ message: 'no kind' })).toBe(false);
    expect(isCommandError(null)).toBe(false);
  });
});
