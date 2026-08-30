import { forwardRef, type HTMLAttributes } from 'react';
import { cn } from '../lib/cn';
import { useReducedMotion } from '../lib/useReducedMotion';

const SIZES = {
  sm: 'size-3.5 border-[1.5px]',
  md: 'size-5 border-2',
  lg: 'size-8 border-2',
} as const;

export interface SpinnerProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className'> {
  readonly size?: keyof typeof SIZES;
  readonly className?: string;
  /**
   * Accessible name, supplied by the caller so no English lives in this
   * package. When omitted the spinner is hidden from assistive technology,
   * which is correct for a spinner inside an already-labelled control.
   */
  readonly label?: string;
}

/**
 * An indeterminate activity indicator.
 *
 * Under `prefers-reduced-motion` the rotation is replaced by a static ring
 * rather than removed entirely: a vestibular-sensitive user still needs to
 * know that something is in progress, they just must not be made dizzy by it.
 */
export const Spinner = forwardRef<HTMLDivElement, SpinnerProps>(function Spinner(
  { size = 'md', className, label, ...rest },
  ref,
) {
  const reduced = useReducedMotion();

  return (
    <div
      ref={ref}
      role={label === undefined ? undefined : 'status'}
      aria-label={label}
      aria-hidden={label === undefined ? true : undefined}
      data-reduced-motion={reduced || undefined}
      className={cn(
        'inline-block shrink-0 rounded-full border-current border-t-transparent',
        reduced ? 'opacity-60' : 'animate-spin',
        SIZES[size],
        className,
      )}
      {...rest}
    />
  );
});
