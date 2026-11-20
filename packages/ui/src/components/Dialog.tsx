import { Dialog as RadixDialog } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { focusRing, overlayMotion, overlayScrim } from '../lib/styles';

export const DialogRoot = RadixDialog.Root;
export const DialogTrigger = RadixDialog.Trigger;
export const DialogClose = RadixDialog.Close;

/**
 * Maximum width per size, as explicit lengths rather than `max-w-2xl`/`max-w-3xl`.
 *
 * Tailwind resolves those through its `--container-*` scale, and the desktop
 * app redefines the ultrawide end of that scale for its `@3xl/main:` container
 * variants (`--container-3xl: 120rem`, apps/desktop/src/styles.css). A dialog
 * asking for `max-w-3xl` therefore asked for 1920 px and stretched across a
 * fullscreen window — the whole reason this map exists. A cap has to be a
 * length for it to mean what it says, and the primitives cannot know which
 * tokens the app around them has redefined.
 */
const SIZES = {
  sm: 'max-w-[24rem]',
  md: 'max-w-[28rem]',
  lg: 'max-w-[32rem]',
  xl: 'max-w-[42rem]',
  '2xl': 'max-w-[48rem]',
} as const;

/**
 * The widest dialog the design system defines, in rem.
 *
 * Exported so a test can assert that no size exceeds it: a cap nobody wrote
 * down is a cap that drifts upwards one convenience at a time.
 */
export const DIALOG_MAX_WIDTH_REM = 48;

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
            overlayScrim,
          )}
        />
        <RadixDialog.Content
          ref={ref}
          className={cn(
            // Both axes are capped to the window (`dvh`/`vw`: the webview's real viewport) and the
            // body is the only part that scrolls. Without the height cap a long dialog (drive
            // clean-up, settings) grew past the window edge and its header/footer - the Close and
            // Next buttons - were cut off; without the width cap a fullscreen window stretched the
            // dialog into a 1920 px wide sheet of running text.
            'fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden',
            'rounded-[var(--radius-widget)] border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] shadow-[var(--shadow-overlay)]',
            overlayMotion,
            SIZES[size],
            className,
          )}
          {...rest}
        >
          <div className="flex shrink-0 items-start gap-3 border-b border-[var(--color-border-subtle)] px-4 py-3">
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

          <div
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-3 text-sm"
            data-dialog-body=""
          >
            {children}
          </div>

          {footer !== undefined && (
            <div className="flex shrink-0 items-center justify-end gap-2 border-t border-[var(--color-border-subtle)] px-4 py-3">
              {footer}
            </div>
          )}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    );
  },
);
