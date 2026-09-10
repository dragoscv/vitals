import { Tooltip as RadixTooltip } from 'radix-ui';
import { type ReactElement, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { overlayMotion, overlaySurface } from '../lib/styles';

export interface TooltipProviderProps {
  readonly children: ReactNode;
  /**
   * Delay before the first tooltip in a group appears.
   *
   * 400ms rather than Radix's 700ms: this app is a dense grid of icon-only
   * buttons where the tooltip is often the only label, and three quarters of a
   * second of nothing feels broken.
   */
  readonly delayDuration?: number;
}

/** Mount once near the app root so hovering between controls skips the delay. */
export function TooltipProvider({ children, delayDuration = 400 }: TooltipProviderProps) {
  return (
    <RadixTooltip.Provider delayDuration={delayDuration} skipDelayDuration={300}>
      {children}
    </RadixTooltip.Provider>
  );
}

export interface TooltipProps {
  /** A single focusable element. Radix clones it to attach the trigger props. */
  readonly children: ReactElement;
  /** Tooltip text. Caller-supplied; no English is baked into this package. */
  readonly content: ReactNode;
  readonly side?: 'top' | 'right' | 'bottom' | 'left';
  readonly align?: 'start' | 'center' | 'end';
  readonly className?: string;
  /** Controlled open state, for tours and forced-visible states. */
  readonly open?: boolean;
  readonly onOpenChange?: (open: boolean) => void;
}

/**
 * A hover/focus label for a control.
 *
 * Never the only place information lives. A tooltip is unavailable to touch
 * users and disappears the moment the pointer moves, so anything essential —
 * such as why an action is disabled — must also exist in the persistent UI.
 *
 * Radix supplies the parts that are easy to get wrong: the tooltip opens on
 * keyboard focus as well as hover (WCAG 1.4.13), dismisses on Escape without
 * moving focus, and is wired to the trigger with `aria-describedby` rather
 * than being an orphaned floating div.
 */
export function Tooltip({
  children,
  content,
  side = 'top',
  align = 'center',
  className,
  open,
  onOpenChange,
}: TooltipProps) {
  return (
    <RadixTooltip.Root
      {...(open !== undefined && { open })}
      {...(onOpenChange !== undefined && { onOpenChange })}
    >
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          side={side}
          align={align}
          sideOffset={6}
          // Collision padding keeps the tooltip off the window chrome; a
          // tooltip clipped by the title bar is worse than none.
          collisionPadding={8}
          className={cn(
            overlaySurface,
            overlayMotion,
            'max-w-64 px-2 py-1 text-2xs text-[var(--color-fg-default)]',
            className,
          )}
        >
          {content}
          <RadixTooltip.Arrow className="fill-[var(--color-bg-raised)]" width={10} height={5} />
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}
