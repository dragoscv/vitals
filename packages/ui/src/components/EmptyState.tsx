import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';

export interface EmptyStateProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className'> {
  readonly className?: string;
  readonly icon?: ReactNode;
  /** All copy comes from the caller; this package ships no user-facing text. */
  readonly title: string;
  readonly description?: string;
  /** Typically a Button. Present so the state is a next step, not a dead end. */
  readonly action?: ReactNode;
}

/**
 * Shown when a list or panel has nothing to display.
 *
 * `role="status"` so that filtering a process list down to zero results is
 * announced. Without it a keyboard user types into the search box, the table
 * silently empties, and nothing tells them why their arrow keys stopped doing
 * anything.
 */
export const EmptyState = forwardRef<HTMLDivElement, EmptyStateProps>(function EmptyState(
  { className, icon, title, description, action, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      role="status"
      className={cn(
        'flex flex-col items-center justify-center gap-2 px-6 py-10 text-center',
        className,
      )}
      {...rest}
    >
      {icon !== undefined && (
        <span aria-hidden="true" className="text-[var(--color-fg-subtle)] [&_svg]:size-8">
          {icon}
        </span>
      )}
      <p className="text-sm font-medium text-[var(--color-fg-default)]">{title}</p>
      {description !== undefined && (
        <p className="text-2xs max-w-sm text-[var(--color-fg-muted)]">{description}</p>
      )}
      {action !== undefined && <div className="mt-2">{action}</div>}
    </div>
  );
});
