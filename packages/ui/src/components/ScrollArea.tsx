import { ScrollArea as Radix } from 'radix-ui';
import { forwardRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface ScrollAreaProps {
  readonly className?: string;
  readonly viewportClassName?: string;
  readonly orientation?: 'vertical' | 'horizontal' | 'both';
  readonly children: ReactNode;
  /**
   * Names the scrollable region.
   *
   * Radix makes the viewport focusable so it can be scrolled with the keyboard.
   * A focusable element with no accessible name is announced as a bare "group",
   * which tells the user nothing about what they have just landed in.
   */
  readonly ariaLabel?: string;
}

function Bar({ orientation }: { readonly orientation: 'vertical' | 'horizontal' }) {
  return (
    <Radix.Scrollbar
      orientation={orientation}
      className={cn(
        'duration-(--duration-fast) flex touch-none select-none p-0.5 transition-colors',
        orientation === 'vertical' ? 'w-2.5' : 'h-2.5 flex-col',
        'hover:bg-[var(--color-bg-inset)]',
      )}
    >
      <Radix.Thumb className="relative flex-1 rounded-full bg-[var(--color-border-default)] hover:bg-[var(--color-border-strong)]" />
    </Radix.Scrollbar>
  );
}

/**
 * A scrollable region with a themed scrollbar.
 *
 * Only for chrome — sidebars, panels, menus. **Never wrap a virtualised
 * process table in this**: Radix replaces the native scrollbar with a custom
 * one whose thumb is sized from the rendered content, and a virtualiser
 * renders twenty of five hundred rows, so the thumb would report the wrong
 * proportion and jump as rows recycle. Long data lists keep the native
 * scrollbar, which theme.css already styles.
 *
 * `type="hover"` keeps the bar out of the way but still exposes it on hover
 * and while scrolling; an always-hidden scrollbar removes the only cue that a
 * region has more content, which is a 1.3.1 problem as well as a usability one.
 */
export const ScrollArea = forwardRef<ElementRef<typeof Radix.Viewport>, ScrollAreaProps>(
  function ScrollArea(
    { className, viewportClassName, orientation = 'vertical', children, ariaLabel },
    ref,
  ) {
    return (
      <Radix.Root type="hover" scrollHideDelay={600} className={cn('overflow-hidden', className)}>
        <Radix.Viewport
          ref={ref}
          aria-label={ariaLabel}
          className={cn('size-full [&>div]:!block', viewportClassName)}
        >
          {children}
        </Radix.Viewport>
        {orientation !== 'horizontal' && <Bar orientation="vertical" />}
        {orientation !== 'vertical' && <Bar orientation="horizontal" />}
        <Radix.Corner />
      </Radix.Root>
    );
  },
);
