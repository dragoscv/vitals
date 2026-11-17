import { beforeEach, describe, expect, it } from 'vitest';

import { runBackInterceptor, setBackHandler } from './back';

describe('the shell’s Back interceptor', () => {
  beforeEach(() => setBackHandler(null));

  it('reports the press unhandled when no screen is holding Back, so the shell pops the screen', () => {
    expect(runBackInterceptor()).toBe(false);
  });

  it('lets the mounted screen consume the press, and stops asking once it unmounts', () => {
    let stepped = 0;
    setBackHandler(() => {
      stepped++;
      return true;
    });
    expect(runBackInterceptor()).toBe(true);
    expect(stepped).toBe(1);

    setBackHandler(null);
    expect(runBackInterceptor()).toBe(false);
    expect(stepped).toBe(1);
  });
});
