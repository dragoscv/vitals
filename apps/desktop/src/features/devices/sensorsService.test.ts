import { describe, expect, it } from 'vitest';

import { serviceAction, type SensorsServiceStatus } from './sensorsService';

const base: SensorsServiceStatus = {
  installed: false,
  running: false,
  error: null,
  pawnioInstalled: false,
  helperAvailable: true,
};

describe('serviceAction', () => {
  it('offers nothing before the status is known', () => {
    expect(serviceAction(null)).toBe('none');
  });

  it('offers install only when the helper ships', () => {
    expect(serviceAction(base)).toBe('install');
    expect(serviceAction({ ...base, helperAvailable: false })).toBe('unavailable');
  });

  it('always offers removal once installed, reading or not', () => {
    // A service that cannot read this CPU must never be left running with no
    // way to take it off from the app that put it there.
    expect(serviceAction({ ...base, installed: true, running: true })).toBe('remove');
    expect(serviceAction({ ...base, installed: true, running: false })).toBe('remove');
    expect(serviceAction({ ...base, installed: true, helperAvailable: false })).toBe('remove');
  });
});
