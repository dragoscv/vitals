import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from './ContextMenu';

afterEach(cleanup);

function Fixture({ onKill = vi.fn() }: { readonly onKill?: () => void }) {
  return (
    <ContextMenu>
      <ContextMenuTrigger data-testid="row">chrome.exe</ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem>Open file location</ContextMenuItem>
        <ContextMenuItem destructive onSelect={onKill}>
          End task
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

function openWithMouse() {
  fireEvent.contextMenu(screen.getByTestId('row'), { button: 2 });
}

describe('ContextMenu', () => {
  it('opens on right-click', async () => {
    render(<Fixture />);
    openWithMouse();
    await act(async () => {});

    expect(screen.getByRole('menu'), 'right-click did not open the menu').toBeTruthy();
  });

  it('is reachable without a mouse — this is how a keyboard user kills a process', async () => {
    render(<Fixture />);
    const row = screen.getByTestId('row');

    // The Menu key and Shift+F10 do not arrive as a keydown the component can
    // see — the browser turns them into a native `contextmenu` event with no
    // pointer coordinates (detail 0, clientX/Y 0). Asserting on that shape is
    // what proves keyboard invocation works; a handler that gated on a mouse
    // button or a non-zero position would pass the right-click test above and
    // still leave keyboard users with no route to row actions at all.
    fireEvent.contextMenu(row, { detail: 0, clientX: 0, clientY: 0, button: 0 });
    await act(async () => {});

    expect(
      screen.queryByRole('menu'),
      'the context menu could not be opened from the keyboard — row actions are mouse-only',
    ).not.toBeNull();
  });

  it('moves the highlight with arrow keys rather than requiring hover', async () => {
    render(<Fixture />);
    openWithMouse();
    await act(async () => {});

    const menu = screen.getByRole('menu');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    await act(async () => {});

    const items = screen.getAllByRole('menuitem');
    expect(
      items.some((item) => item.getAttribute('data-highlighted') !== null),
      'arrow keys did not highlight an item — keyboard navigation is invisible',
    ).toBe(true);
  });

  it('runs the selected action', async () => {
    const onKill = vi.fn();
    render(<Fixture onKill={onKill} />);
    openWithMouse();
    await act(async () => {});

    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    await act(async () => {});

    expect(onKill, 'selecting the item did not invoke its handler').toHaveBeenCalledTimes(1);
  });

  it('closes on Escape without running anything', async () => {
    const onKill = vi.fn();
    render(<Fixture onKill={onKill} />);
    openWithMouse();
    await act(async () => {});

    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    await act(async () => {});

    expect(screen.queryByRole('menu'), 'Escape did not dismiss the menu').toBeNull();
    expect(
      onKill,
      'dismissing the menu triggered a destructive action — this would kill a process by accident',
    ).not.toHaveBeenCalled();
  });
});
