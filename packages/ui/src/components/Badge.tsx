import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';

const badge = cva(
  'inline-flex shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-2xs font-medium whitespace-nowrap',
  {
    variants: {
      /**
       * Status variants carry meaning, so each one must also be distinguishable
       * without colour (WCAG 1.4.1). Callers pair them with an icon or text —
       * "Running" is not a green dot, it is a green dot next to the word.
       */
      tone: {
        neutral:
          'border-[var(--color-border-default)] bg-[var(--color-bg-subtle)] text-[var(--color-fg-muted)]',
        accent:
          'border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)] text-[var(--color-accent)]',
        ok: 'border-[var(--color-status-ok)]/40 bg-[var(--color-status-ok)]/12 text-[var(--color-status-ok)]',
        warn: 'border-[var(--color-status-warn)]/40 bg-[var(--color-status-warn)]/12 text-[var(--color-status-warn)]',
        danger:
          'border-[var(--color-status-danger)]/40 bg-[var(--color-status-danger)]/12 text-[var(--color-status-danger)]',
        info: 'border-[var(--color-status-info)]/40 bg-[var(--color-status-info)]/12 text-[var(--color-status-info)]',
      },
    },
    defaultVariants: { tone: 'neutral' },
  },
);

export interface BadgeProps
  extends Omit<HTMLAttributes<HTMLSpanElement>, 'className'>, VariantProps<typeof badge> {
  readonly className?: string;
  readonly icon?: ReactNode;
  readonly children?: ReactNode;
}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(function Badge(
  { tone, className, icon, children, ...rest },
  ref,
) {
  return (
    <span ref={ref} className={cn(badge({ tone }), className)} {...rest}>
      {icon !== undefined && (
        <span aria-hidden="true" className="contents [&_svg]:size-3">
          {icon}
        </span>
      )}
      {children}
    </span>
  );
});
