import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger } from '@vitals/ui';

import { useRowMenu } from './useRowMenu';

function List() {
  const menu = useRowMenu();
  return (
    <div role="list" tabIndex={0} onKeyDown={menu.onKeyDown}>
      {['a', 'b'].map((id) => (
        <ContextMenu key={id} {...menu.rootProps(id)}>
          <ContextMenuTrigger asChild>
            <div role="listitem" data-testid={`row-${id}`} onContextMenu={menu.onContextMenu}>
              {id}
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem>Act on {id}</ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      ))}
    </div>
  );
}

describe('useRowMenu', () => {
  it('opens on a right click', async () => {
    render(<List />);
    fireEvent.contextMenu(screen.getByTestId('row-a'), { button: 2, clientX: 5, clientY: 5 });
    await act(async () => {});
    expect(await screen.findByRole('menu')).toBeTruthy();
  });

  it('refuses a contextmenu that is neither a right click nor the menu key', async () => {
    render(<List />);
    fireEvent.contextMenu(screen.getByTestId('row-a'), { button: 0, clientX: 0, clientY: 0 });
    await act(async () => {});
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('opens from Shift+F10 on the list', async () => {
    render(<List />);
    const row = screen.getByTestId('row-b');
    fireEvent.keyDown(screen.getByRole('list'), { key: 'F10', shiftKey: true });
    fireEvent.contextMenu(row, { button: 0, clientX: 0, clientY: 0 });
    await act(async () => {});
    expect(await screen.findByRole('menu')).toBeTruthy();
  });
});
