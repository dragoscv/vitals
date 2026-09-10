import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Toaster, toast } from './Toaster';

afterEach(() => {
  toast.dismiss();
  cleanup();
  document.documentElement.classList.remove('dark');
  delete document.documentElement.dataset['reduceMotion'];
});

/** sonner commits a queued toast on a timer, not synchronously with the call. */
async function mountWithToast(kind: 'default' | 'success' = 'default') {
  const result = render(<Toaster regionLabel="Notificări" closeLabel="Închide notificarea" />);
  await act(async () => {
    if (kind === 'success') toast.success('Saved');
    else toast('Saved');
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
  });
  return result;
}

const toaster = () => document.querySelector('[data-sonner-toaster]');

describe('Toaster', () => {
  it('names the region and the close button with the caller-supplied translations, not sonner’s English', async () => {
    await mountWithToast();

    expect(screen.getByRole('region', { name: /Notificări/ })).toBeTruthy();
    expect(screen.queryByRole('region', { name: /Notifications/ })).toBeNull();

    // Auto-dismiss alone strands anyone reading slowly or interrupted: the
    // only way back to the message would be to reproduce whatever caused it.
    expect(screen.getByRole('button', { name: 'Închide notificarea' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Close toast' })).toBeNull();
  });

  it('follows the document’s resolved colour mode rather than the OS media query', async () => {
    document.documentElement.classList.add('dark');
    await mountWithToast();

    expect(toaster()?.getAttribute('data-sonner-theme')).toBe('dark');
  });

  it('switches mode when the root class changes after mount, so a theme change does not leave one light surface', async () => {
    await mountWithToast();
    expect(toaster()?.getAttribute('data-sonner-theme')).toBe('light');

    document.documentElement.classList.add('dark');
    // MutationObserver callbacks are delivered as microtasks.
    await act(async () => {
      await Promise.resolve();
    });

    expect(toaster()?.getAttribute('data-sonner-theme')).toBe('dark');
  });

  it('goes still when the app’s own reduce-motion override is set, which no media query can see', async () => {
    document.documentElement.dataset['reduceMotion'] = 'true';
    await mountWithToast();

    expect(toaster()?.classList.contains('vitals-toaster-still')).toBe(true);
  });

  it('animates normally when nothing has asked for reduced motion', async () => {
    await mountWithToast();
    expect(toaster()?.classList.contains('vitals-toaster-still')).toBe(false);
  });

  it('does not use sonner’s rich palette, since a literal colour ignores all three theme axes', async () => {
    await mountWithToast('success');

    const item = document.querySelector('[data-sonner-toast]');
    expect(item?.getAttribute('data-rich-colors')).toBe('false');
    // Every surface colour is a token, so the accent and translucency
    // settings reach the toast like everything else.
    expect(item?.className).toContain('bg-[var(--color-bg-raised)]');
  });
});
