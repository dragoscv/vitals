import { Dialog as RadixDialog } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { focusRing } from '../lib/styles';

export const DialogRoot = RadixDialog.Root;
export const DialogTrigger = RadixDialog.Trigger;
export const DialogClose = RadixDialog.Close;

const SIZES = { sm: 'max-w-sm', md: 'max-w-md', lg: 'max-w-lg', xl: 'max-w-2xl' } as const;

export interface DialogContentProps extends Omit<
  ComponentPropsWithoutRef<typeof RadixDialog.Content>,
  'className' | 'title'
> {
  readonly className?: string;
  readonly size?: keyof typeof SIZES;
  /**
   * The accessible title. Required, because a dialog with no name is announced
   * as "dialog" and gives a screen reader user nothing to orient on.
   */
  readonly title: string;
  /** Hide the title visually while keeping it for assistive technology. */
  readonly hideTitle?: boolean;
  readonly description?: string;
  /** Accessible name for the close button — caller supplies the translation. */
  readonly closeLabel: string;
  readonly footer?: ReactNode;
  readonly children?: ReactNode;
}

/**
 * A modal dialog.
 *
 * The behaviours below are why this delegates to Radix rather than being
 * hand-rolled, and all of them are load-bearing for a keyboard-only user:
 *
 * - Focus is trapped inside while open and **returned to the trigger** on
 *   close. Without the return, dismissing a confirmation drops the user at the
 *   top of the document and they must tab back through the entire process
 *   table to reach the row they were acting on.
 * - Escape closes; the outside click and Escape paths run the same handler, so
 *   a caller cannot accidentally support one and not the other.
 * - Everything outside the dialog is marked `aria-hidden`, which is the only
 *   thing that actually stops a screen reader's virtual cursor from wandering
 *   out of a modal — a focus trap alone does not.
 */
export const DialogContent = forwardRef<ElementRef<typeof RadixDialog.Content>, DialogContentProps>(
  function DialogContent(
    {
      className,
      size = 'md',
      title,
      hideTitle = false,
      description,
      closeLabel,
      footer,
      children,
      ...rest
    },
    ref,
  ) {
    return (
      <RadixDialog.Portal>
        <RadixDialog.Overlay
          className={cn(
            'fixed inset-0 z-50 bg-[var(--color-bg-inset)]/70 backdrop-blur-[2px]',
            'transition-opacity duration-(--duration-fast) data-[state=closed]:opacity-0 data-[state=open]:opacity-100',
          )}
        />
        <RadixDialog.Content
          ref={ref}
          className={cn(
            'fixed top-1/2 left-1/2 z-50 flex w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col',
            'rounded-[var(--radius-widget)] border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] shadow-[var(--shadow-overlay)]',
            'transition-opacity duration-(--duration-fast) data-[state=closed]:opacity-0 data-[state=open]:opacity-100',
            SIZES[size],
            className,
          )}
          {...rest}
        >
          <div className="flex items-start gap-3 border-b border-[var(--color-border-subtle)] px-4 py-3">
            <div className="min-w-0 flex-1">
              <RadixDialog.Title
                className={cn(
                  'text-sm font-semibold',
                  hideTitle && 'sr-only absolute size-px overflow-hidden whitespace-nowrap',
                )}
              >
                {title}
              </RadixDialog.Title>
              {description !== undefined && (
                <RadixDialog.Description className="mt-1 text-2xs text-[var(--color-fg-muted)]">
                  {description}
                </RadixDialog.Description>
              )}
            </div>
            <RadixDialog.Close
              aria-label={closeLabel}
              className={cn(
                'inline-flex size-6 shrink-0 items-center justify-center rounded-[var(--radius-control)] text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-subtle)] hover:text-[var(--color-fg-default)]',
                focusRing,
              )}
            >
              {/* Inline glyph rather than an icon import: the close affordance
                  must never fail to render because an icon set was tree-shaken. */}
              <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden="true" fill="none">
                <path
                  d="M3.5 3.5l9 9m0-9l-9 9"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            </RadixDialog.Close>
          </div>

          <div className="min-h-0 overflow-auto px-4 py-3 text-sm">{children}</div>

          {footer !== undefined && (
            <div className="flex items-center justify-end gap-2 border-t border-[var(--color-border-subtle)] px-4 py-3">
              {footer}
            </div>
          )}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    );
  },
);
