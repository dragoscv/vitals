import { describe, expect, it } from 'vitest';

import { errorMessage, isCommandError, isMissingCommand } from './commandError';

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

describe('a command this build does not have', () => {
  it('says the feature is unsupported rather than echoing Tauri at the user', () => {
    // Tauri rejects an `invoke` for a command outside `generate_handler!` with
    // this exact bare string; showing it verbatim tells the user nothing.
    const rejection = 'Command set_process_affinity not found';
    expect(isMissingCommand(rejection)).toBe(true);
    expect(errorMessage(rejection, { unsupported: 'Not available on this system' })).toBe(
      'Not available on this system',
    );
  });

  it('leaves a real failure untouched even when a fallback is offered', () => {
    expect(errorMessage('open process 6440 requires elevation', { unsupported: 'nope' })).toBe(
      'open process 6440 requires elevation',
    );
    // A handler's own refusal is a `kind: unsupported`, not a missing command,
    // and it carries the more specific message that must win.
    expect(errorMessage({ kind: 'unsupported', message: 'revealing a file' })).toBe(
      'revealing a file',
    );
  });

  it('does not mistake an ordinary error for a missing command', () => {
    expect(isMissingCommand(new Error('boom'))).toBe(false);
    expect(isMissingCommand(null)).toBe(false);
  });
});
