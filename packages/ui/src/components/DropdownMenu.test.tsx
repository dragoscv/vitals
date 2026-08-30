import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './DropdownMenu';
import { Button } from './Button';

afterEach(cleanup);

function Fixture({ onSelect = vi.fn() }: { readonly onSelect?: () => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button>Columns</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem onSelect={onSelect}>Reset</DropdownMenuItem>
        <DropdownMenuCheckboxItem checked>PID</DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

describe('DropdownMenu', () => {
  it('reports its expanded state on the trigger', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Columns' });

    expect(
      trigger.getAttribute('aria-expanded'),
      'a collapsed menu trigger must report aria-expanded="false"',
    ).toBe('false');

    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: 'mouse' });
    fireEvent.click(trigger);
    await act(async () => {});

    expect(
      trigger.getAttribute('aria-expanded'),
      'the trigger did not update aria-expanded when the menu opened',
    ).toBe('true');
  });

  it('opens from the keyboard with Enter', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Columns' });
    trigger.focus();

    fireEvent.keyDown(trigger, { key: 'Enter' });
    await act(async () => {});

    expect(
      screen.queryByRole('menu'),
      'Enter did not open the menu from the trigger',
    ).not.toBeNull();
  });

  it('returns focus to the trigger on Escape, so the toolbar position is not lost', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Columns' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    await act(async () => {});

    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    await act(async () => {});

    // Radix restores focus when the focus scope unmounts, which happens after
    // the exit transition rather than synchronously on the keypress.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(
      document.activeElement,
      'focus was not returned to the trigger; the user must tab back through the toolbar',
    ).toBe(trigger);
  });

  it('exposes checkable items as menuitemcheckbox with their state', async () => {
    render(<Fixture />);
    fireEvent.keyDown(screen.getByRole('button', { name: 'Columns' }), { key: 'Enter' });
    await act(async () => {});

    const item = screen.getByRole('menuitemcheckbox', { name: 'PID' });
    expect(
      item.getAttribute('aria-checked'),
      'a checked column toggle did not report aria-checked — its state is invisible to a screen reader',
    ).toBe('true');
  });
});
