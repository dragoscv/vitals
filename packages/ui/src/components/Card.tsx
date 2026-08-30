import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface CardProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className' | 'title'> {
  readonly className?: string;
  /**
   * Turns the card into a labelled landmark region.
   *
   * A dashboard is a wall of similar-looking panels; without a name a screen
   * reader user has no way to tell the CPU widget from the GPU widget except
   * by reading the contents of each.
   */
  readonly regionLabel?: string;
  readonly children?: ReactNode;
}

/**
 * The surface every widget and panel sits on.
 *
 * Deliberately a plain container rather than a slot-based `Card.Header` /
 * `Card.Body` family: dashboard widgets vary too much for a fixed skeleton,
 * and the compound API would be worked around within a week.
 */
export const Card = forwardRef<HTMLDivElement, CardProps>(function Card(
  { className, regionLabel, children, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      {...(regionLabel !== undefined && { role: 'region', 'aria-label': regionLabel })}
      className={cn(
        'rounded-[var(--radius-widget)] border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] shadow-[var(--shadow-widget)]',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
});

export interface CardHeaderProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className'> {
  readonly className?: string;
  /** Placed at the trailing edge — menus, refresh buttons, live badges. */
  readonly actions?: ReactNode;
  readonly children?: ReactNode;
}

export const CardHeader = forwardRef<HTMLDivElement, CardHeaderProps>(function CardHeader(
  { className, actions, children, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      className={cn(
        'flex min-h-9 items-center justify-between gap-2 border-b border-[var(--color-border-subtle)] px-3 py-2',
        className,
      )}
      {...rest}
    >
      <div className="min-w-0 flex-1">{children}</div>
      {actions !== undefined && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
    </div>
  );
});

export interface CardTitleProps extends Omit<HTMLAttributes<HTMLHeadingElement>, 'className'> {
  readonly className?: string;
  /**
   * Heading rank. Exposed because the correct level depends on where the card
   * sits in the page outline, and a component that hardcodes `h3` produces a
   * skipped-heading-level failure the moment it is nested.
   */
  readonly level?: 2 | 3 | 4 | 5 | 6;
  readonly children?: ReactNode;
}

export const CardTitle = forwardRef<HTMLHeadingElement, CardTitleProps>(function CardTitle(
  { className, level = 3, children, ...rest },
  ref,
) {
  const Heading = `h${level}` as const;
  return (
    <Heading
      ref={ref}
      className={cn('truncate text-sm font-semibold text-[var(--color-fg-default)]', className)}
      {...rest}
    >
      {children}
    </Heading>
  );
});

export interface CardBodyProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className'> {
  readonly className?: string;
  readonly children?: ReactNode;
}

export const CardBody = forwardRef<HTMLDivElement, CardBodyProps>(function CardBody(
  { className, children, ...rest },
  ref,
) {
  return (
    <div ref={ref} className={cn('p-3', className)} {...rest}>
      {children}
    </div>
  );
});
