import { forwardRef, useId, type InputHTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledControl } from '../lib/styles';

export interface InputProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  'className' | 'size'
> {
  readonly className?: string;
  readonly inputClassName?: string;
  readonly label?: ReactNode;
  /** Accessible name when no visible `label` is rendered. */
  readonly ariaLabel?: string;
  readonly leadingIcon?: ReactNode;
  readonly trailingSlot?: ReactNode;
  /** Help text. Suppressed while `error` is set so the two never both speak. */
  readonly hint?: string;
  /**
   * Validation message.
   *
   * Setting it does three things at once — `aria-invalid`, an
   * `aria-describedby` link to the message, and a live region on the message —
   * because a red border alone is invisible to a screen reader and fails 1.4.1.
   */
  readonly error?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  {
    className,
    inputClassName,
    label,
    ariaLabel,
    leadingIcon,
    trailingSlot,
    hint,
    error,
    id,
    disabled,
    ...rest
  },
  ref,
) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const messageId = `${inputId}-message`;
  const message = error ?? hint;

  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {label !== undefined && (
        <label htmlFor={inputId} className="text-2xs font-medium text-[var(--color-fg-muted)]">
          {label}
        </label>
      )}

      <div
        className={cn(
          'flex h-8 items-center gap-1.5 rounded-[var(--radius-control)] border bg-[var(--color-bg-inset)] px-2',
          'duration-(--duration-fast) transition-colors',
          error === undefined
            ? 'border-[var(--color-border-default)]'
            : 'border-[var(--color-status-danger)]',
          // The ring is drawn on the wrapper, not the input, so the icons and
          // trailing slot sit inside the focus indicator rather than outside it
          // — an indicator that excludes part of the control it describes fails
          // 2.4.11.
          'focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-[var(--color-accent)]',
          disabled === true && 'cursor-not-allowed opacity-50',
        )}
      >
        {leadingIcon !== undefined && (
          <span
            aria-hidden="true"
            className="shrink-0 text-[var(--color-fg-subtle)] [&_svg]:size-3.5"
          >
            {leadingIcon}
          </span>
        )}
        <input
          ref={ref}
          id={inputId}
          disabled={disabled}
          aria-label={label === undefined ? ariaLabel : undefined}
          aria-invalid={error === undefined ? undefined : true}
          aria-describedby={message === undefined ? undefined : messageId}
          className={cn(
            'min-w-0 flex-1 select-text bg-transparent text-sm text-[var(--color-fg-default)] outline-none',
            'placeholder:text-[var(--color-fg-subtle)]',
            disabledControl,
            inputClassName,
          )}
          {...rest}
        />
        {trailingSlot !== undefined && <span className="shrink-0">{trailingSlot}</span>}
      </div>

      {message !== undefined && (
        <p
          id={messageId}
          // Only the error path is live. Announcing static help text on every
          // render would talk over the user as they type.
          {...(error !== undefined && { role: 'alert', 'aria-live': 'polite' as const })}
          className={cn(
            'text-2xs',
            error === undefined
              ? 'text-[var(--color-fg-subtle)]'
              : 'text-[var(--color-status-danger)]',
          )}
        >
          {message}
        </p>
      )}
    </div>
  );
});
