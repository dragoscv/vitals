import { describe, expect, it } from 'vitest';

import { displayName, matchesProcessName } from './process';

describe('displayName', () => {
  it('uses the name the app gives itself when it has one', () => {
    expect(
      displayName({ name: 'Code - Insiders.exe', description: 'Visual Studio Code - Insiders' }),
    ).toBe('Visual Studio Code - Insiders');
  });

  it('falls back to the file name when there is no description, or only whitespace', () => {
    expect(displayName({ name: 'svchost.exe', description: null })).toBe('svchost.exe');
    expect(displayName({ name: 'svchost.exe', description: '   ' })).toBe('svchost.exe');
  });
});

describe('matchesProcessName', () => {
  it('finds a process by either its app name or its file name', () => {
    const p = { name: 'explorer.exe', description: 'Windows Explorer' };
    expect(matchesProcessName(p, 'windows')).toBe(true);
    expect(matchesProcessName(p, 'EXPLORER.EXE')).toBe(true);
    expect(matchesProcessName(p, 'chrome')).toBe(false);
  });
});
