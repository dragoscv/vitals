import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledControl, focusRing } from '../lib/styles';
import { Spinner } from './Spinner';

const button = cva(
  cn(
    'inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-[var(--radius-control)] font-medium select-none',
    'transition-colors duration-(--duration-fast) ease-(--ease-out-quart)',
    focusRing,
    disabledControl,
  ),
  {
    variants: {
      variant: {
        primary:
          'bg-[var(--color-accent)] text-[var(--color-fg-on-accent)] hover:bg-[var(--color-accent-hover)]',
        secondary:
          'border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] hover:bg-[var(--color-bg-subtle)]',
        ghost:
          'bg-transparent text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-subtle)] hover:text-[var(--color-fg-default)]',
        // Destructive actions in this app are irreversible — terminating a
        // process loses unsaved work. The variant is visually loud on purpose.
        danger:
          'bg-[var(--color-status-danger)] text-[var(--color-fg-on-accent)] hover:brightness-110',
      },
      size: {
        sm: 'h-7 px-2.5 text-2xs',
        md: 'h-8 px-3 text-sm',
        lg: 'h-10 px-4 text-sm',
      },
      fullWidth: { true: 'w-full', false: '' },
    },
    defaultVariants: { variant: 'secondary', size: 'md', fullWidth: false },
  },
);

type ButtonVariants = Required<Pick<VariantProps<typeof button>, 'variant' | 'size'>>;

export interface ButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'className'>, Partial<ButtonVariants> {
  readonly className?: string;
  readonly fullWidth?: boolean;
  /** Rendered before the label. Must be decorative; the label carries meaning. */
  readonly leadingIcon?: ReactNode;
  readonly trailingIcon?: ReactNode;
  /**
   * Shows a spinner and blocks activation.
   *
   * Separate from `disabled` because the two mean different things to assistive
   * technology: `aria-busy` says "this is working", `aria-disabled` says "you
   * may not do this". Conflating them makes a submitting form sound permanently
   * unavailable.
   */
  readonly loading?: boolean;
  /**
   * Announced while `loading`. Required when `loading` can become true so the
   * component never has to invent English of its own.
   */
  readonly loadingLabel?: string;
  readonly children?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant,
    size,
    fullWidth,
    className,
    leadingIcon,
    trailingIcon,
    loading = false,
    loadingLabel,
    disabled = false,
    children,
    type = 'button',
    ...rest
  },
  ref,
) {
  // `disabled` rather than `aria-disabled` while loading: a half-submitted
  // form must not accept a second click, and users do double-click buttons
  // that appear to be doing nothing.
  const inert = disabled || loading;

  return (
    <button
      ref={ref}
      type={type}
      disabled={inert}
      aria-busy={loading || undefined}
      className={cn(button({ variant, size, fullWidth }), className)}
      {...rest}
    >
      {loading ? (
        <Spinner
          size={size === 'lg' ? 'md' : 'sm'}
          {...(loadingLabel !== undefined && { label: loadingLabel })}
        />
      ) : (
        leadingIcon !== undefined && (
          <span aria-hidden="true" className="contents">
            {leadingIcon}
          </span>
        )
      )}
      {children}
      {!loading && trailingIcon !== undefined && (
        <span aria-hidden="true" className="contents">
          {trailingIcon}
        </span>
      )}
    </button>
  );
});
