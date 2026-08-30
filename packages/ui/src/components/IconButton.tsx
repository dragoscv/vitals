import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledControl, focusRing } from '../lib/styles';
import { Spinner } from './Spinner';

const iconButton = cva(
  cn(
    'inline-flex shrink-0 items-center justify-center rounded-[var(--radius-control)]',
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
        danger:
          'bg-[var(--color-status-danger)] text-[var(--color-fg-on-accent)] hover:brightness-110',
      },
      size: {
        // Toolbar density in a table row genuinely needs 24px, which is below
        // the WCAG 2.2 target-size minimum. It is allowed only because 2.5.8
        // exempts targets with an equivalent elsewhere — every row action is
        // also reachable from the row's context menu, which is full size.
        sm: 'size-6 [&_svg]:size-3.5',
        md: 'size-8 [&_svg]:size-4',
        lg: 'size-10 [&_svg]:size-5',
      },
    },
    defaultVariants: { variant: 'ghost', size: 'md' },
  },
);

export interface IconButtonProps
  extends
    Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'className' | 'aria-label'>,
    VariantProps<typeof iconButton> {
  readonly className?: string;
  readonly icon: ReactNode;
  /**
   * Required, not optional.
   *
   * An icon-only button with no accessible name is announced as "button" and
   * is unusable with a screen reader. Making this a type error is the only
   * reliable way to stop that shipping.
   */
  readonly label: string;
  readonly loading?: boolean;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  {
    variant,
    size,
    className,
    icon,
    label,
    loading = false,
    disabled = false,
    type = 'button',
    ...rest
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      // The visible label lives in the tooltip a caller usually wraps this in;
      // `title` is intentionally not set here because it would duplicate that
      // tooltip and produce a doubled announcement.
      aria-label={label}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn(iconButton({ variant, size }), className)}
      {...rest}
    >
      {loading ? <Spinner size="sm" /> : icon}
    </button>
  );
});
