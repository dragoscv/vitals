import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { InfoPopover } from './InfoPopover';

afterEach(cleanup);

function Fixture() {
  return <InfoPopover label="About hidden devices">Virtual switches start hidden.</InfoPopover>;
}

describe('InfoPopover', () => {
  it('keeps the explanation out of the page until the (i) is pressed', async () => {
    render(<Fixture />);
    expect(screen.queryByText('Virtual switches start hidden.')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'About hidden devices' }));
    await act(async () => {});

    expect(screen.getByText('Virtual switches start hidden.')).toBeTruthy();
  });

  it('tells assistive technology the button opens something', () => {
    // Without aria-expanded an icon button that reveals text is announced as
    // a plain button, and pressing it appears to do nothing.
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'About hidden devices' });
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
  });

  it('closes on Escape and gives focus back to the (i)', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'About hidden devices' });
    trigger.focus();
    fireEvent.click(trigger);
    await act(async () => {});

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    await act(async () => {});
    // Radix restores focus when the focus scope unmounts, after the exit
    // transition rather than synchronously on the keypress.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(trigger);
  });
});
