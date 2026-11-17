/**
 * Every floating surface must carry the paired overlay animation.
 *
 * The stylesheet can define the keyframes perfectly and still leave a menu
 * popping in and out, because the rule only applies to elements that carry the
 * class. This is the test that would catch a surface — the sixth one somebody
 * adds next quarter — that never got it.
 *
 * It counts elements rather than reading computed styles: happy-dom has no CSS
 * engine, so `getComputedStyle(...).animationName` would report whatever the
 * stub felt like. The class is the thing that decides it, so the class is the
 * thing asserted.
 */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Button } from '../components/Button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '../components/ContextMenu';
import { DialogContent, DialogRoot } from '../components/Dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../components/DropdownMenu';
import { InfoPopover } from '../components/InfoPopover';
import { Select } from '../components/Select';
import { Tooltip, TooltipProvider } from '../components/Tooltip';
import { overlayMotion } from './styles';

afterEach(cleanup);

function animatedOverlays(): number {
  return document.querySelectorAll(`.${overlayMotion}`).length;
}

describe('overlay animation coverage', () => {
  it('gives every floating surface the paired animation, so none of them pops in or out', async () => {
    const option = { value: 'auto', label: 'Automatic' } as const;

    const surfaces: readonly (readonly [string, () => Promise<void>])[] = [
      [
        'dialog',
        async () => {
          render(
            <DialogRoot open>
              <DialogContent title="Terminate" closeLabel="Close">
                body
              </DialogContent>
            </DialogRoot>,
          );
          await act(async () => {});
        },
      ],
      [
        'dropdown menu',
        async () => {
          render(
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button>Columns</Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem>Reset</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>,
          );
          const trigger = screen.getByRole('button', { name: 'Columns' });
          trigger.focus();
          fireEvent.keyDown(trigger, { key: 'Enter' });
          await act(async () => {});
        },
      ],
      [
        'context menu',
        async () => {
          render(
            <ContextMenu>
              <ContextMenuTrigger data-testid="row">chrome.exe</ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem>End task</ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>,
          );
          fireEvent.contextMenu(screen.getByTestId('row'), { button: 2 });
          await act(async () => {});
        },
      ],
      [
        'select',
        async () => {
          render(
            <Select
              ariaLabel="Refresh rate"
              value="auto"
              options={[option]}
              onValueChange={vi.fn()}
            />,
          );
          fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
          await act(async () => {});
        },
      ],
      [
        'tooltip',
        async () => {
          render(
            <TooltipProvider delayDuration={0}>
              <Tooltip open content="Refresh now">
                <Button>Refresh</Button>
              </Tooltip>
            </TooltipProvider>,
          );
          await act(async () => {});
        },
      ],
      [
        'info popover',
        async () => {
          render(<InfoPopover label="About hidden devices">explanation</InfoPopover>);
          fireEvent.click(screen.getByRole('button', { name: 'About hidden devices' }));
          await act(async () => {});
        },
      ],
    ];

    for (const [name, open] of surfaces) {
      await open();
      expect(
        animatedOverlays(),
        `the ${name} does not carry ${overlayMotion}: it opens and closes with no animation`,
      ).toBeGreaterThan(0);
      cleanup();
    }
  });
});
