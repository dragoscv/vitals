import { render, screen, waitFor } from '@testing-library/react';
import { motion } from 'motion/react';
import { describe, expect, it, vi } from 'vitest';

import { MotionProvider, reducedMotionFor } from './MotionProvider';
import { MotionRuntime } from './MotionRuntime';

describe('reducedMotionFor', () => {
  it('maps the three-state setting on to Motion\u2019s vocabulary', () => {
    expect(reducedMotionFor(null)).toBe('user');
    expect(reducedMotionFor(true)).toBe('always');
    expect(reducedMotionFor(false)).toBe('never');
  });
});

describe('MotionProvider', () => {
  it('paints its children before Motion has loaded, then keeps them', async () => {
    // The fallback IS the children, so the first frame is never blank and
    // nothing remounts when the chunk lands.
    render(
      <MotionProvider>
        <p>content</p>
      </MotionProvider>,
    );
    expect(screen.getByText('content')).toBeTruthy();
    await waitFor(() => {
      expect(screen.getByText('content')).toBeTruthy();
    });
  });

  it('the runtime refuses a full motion.* component, which is the bundle-size guard', () => {
    // `strict` is what keeps the animation features out of the entry chunk:
    // `motion.div` loads all of them, `m.div` only what LazyMotion supplied.
    // If this stops throwing, someone removed `strict` and the saving is gone.
    const quiet = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() =>
      render(
        <MotionRuntime reducedMotion="user">
          <motion.div />
        </MotionRuntime>,
      ),
    ).toThrow();
    quiet.mockRestore();
  });
});
