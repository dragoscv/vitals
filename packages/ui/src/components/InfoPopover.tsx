import { Popover as RadixPopover } from 'radix-ui';
import { Info } from 'lucide-react';
import { type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { focusRing, overlayMotion, overlaySurface } from '../lib/styles';

export interface InfoPopoverProps {
  /** Names the trigger, e.g. "About hidden devices". Required: it is icon-only. */
  readonly label: string;
  readonly children: ReactNode;
  readonly side?: 'top' | 'right' | 'bottom' | 'left';
  readonly align?: 'start' | 'center' | 'end';
  readonly className?: string;
}

/**
 * An (i) button that opens an explanation on click.
 *
 * A popover rather than a `Tooltip`: the text here is a paragraph the user
 * reads on purpose, and a tooltip vanishes when the pointer moves, is
 * unreachable on touch, and is limited by `max-w-64` to a sentence. Radix
 * gives the rest — opens on Enter/Space, closes on Escape and an outside
 * click, returns focus to the (i), and wires `aria-expanded` /
 * `aria-controls`, so the button announces that it opens something.
 */
export function InfoPopover({
  label,
  children,
  side = 'bottom',
  align = 'start',
  className,
}: InfoPopoverProps) {
  return (
    <RadixPopover.Root>
      <RadixPopover.Trigger
        aria-label={label}
        className={cn(
          'inline-flex size-6 shrink-0 items-center justify-center rounded-full',
          'text-[var(--color-fg-subtle)] hover:bg-[var(--color-bg-subtle)] hover:text-[var(--color-fg-default)]',
          'transition-colors duration-(--duration-fast) ease-(--ease-out-quart)',
          focusRing,
        )}
      >
        <Info aria-hidden="true" className="size-3.5" />
      </RadixPopover.Trigger>
      <RadixPopover.Portal>
        <RadixPopover.Content
          side={side}
          align={align}
          sideOffset={6}
          collisionPadding={8}
          className={cn(
            overlaySurface,
            overlayMotion,
            'max-w-72 px-3 py-2 text-xs leading-relaxed',
            className,
          )}
        >
          {children}
          <RadixPopover.Arrow className="fill-[var(--color-bg-raised)]" width={10} height={5} />
        </RadixPopover.Content>
      </RadixPopover.Portal>
    </RadixPopover.Root>
  );
}
