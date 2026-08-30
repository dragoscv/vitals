import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Tooltip, TooltipProvider } from './Tooltip';
import { IconButton } from './IconButton';

afterEach(cleanup);

function Fixture() {
  return (
    <TooltipProvider delayDuration={0}>
      <Tooltip content="Refresh now">
        <IconButton icon={<svg />} label="Refresh" />
      </Tooltip>
    </TooltipProvider>
  );
}

describe('Tooltip', () => {
  it('opens on keyboard focus, not only on hover (WCAG 1.4.13)', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Refresh' });

    fireEvent.focus(trigger);
    await act(async () => {});

    expect(
      screen.queryAllByText('Refresh now').length,
      'the tooltip never appeared on focus — its content is mouse-only',
    ).toBeGreaterThan(0);
  });

  it('dismisses on Escape while leaving focus on the trigger', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Refresh' });
    trigger.focus();
    fireEvent.focus(trigger);
    await act(async () => {});

    fireEvent.keyDown(document, { key: 'Escape' });
    await act(async () => {});

    expect(
      screen.queryAllByText('Refresh now').length,
      'Escape did not dismiss the tooltip, so it can permanently obscure content',
    ).toBe(0);
    expect(
      document.activeElement,
      'dismissing a tooltip must not move focus — the user is still on the button',
    ).toBe(trigger);
  });

  it('keeps the trigger accessible name distinct from the tooltip text', () => {
    render(<Fixture />);
    // The icon button carries its own label; the tooltip must describe, never
    // replace it. Losing the name is how icon buttons become "button, button".
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeTruthy();
  });
});
