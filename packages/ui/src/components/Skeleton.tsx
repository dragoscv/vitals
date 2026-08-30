import { forwardRef, type HTMLAttributes } from 'react';
import { cn } from '../lib/cn';
import { useReducedMotion } from '../lib/useReducedMotion';

export interface SkeletonProps extends Omit<HTMLAttributes<HTMLDivElement>, 'className'> {
  readonly className?: string;
  readonly shape?: 'text' | 'block' | 'circle';
}

/**
 * A placeholder for content that has not arrived.
 *
 * Always `aria-hidden`. A screen reader gains nothing from a description of a
 * grey rectangle; the loading state belongs on the container, which should
 * carry `aria-busy` and announce the result once via a live region. Marking
 * each skeleton as a status instead produces a burst of announcements that
 * says nothing.
 *
 * The pulse is dropped entirely under reduced motion — unlike a spinner, the
 * shape itself already communicates "loading", so the animation is pure
 * decoration and can go.
 */
export const Skeleton = forwardRef<HTMLDivElement, SkeletonProps>(function Skeleton(
  { className, shape = 'block', ...rest },
  ref,
) {
  const reduced = useReducedMotion();

  return (
    <div
      ref={ref}
      aria-hidden="true"
      className={cn(
        'bg-[var(--color-bg-inset)]',
        reduced ? '' : 'animate-pulse',
        shape === 'text' && 'h-3 rounded-sm',
        shape === 'block' && 'rounded-[var(--radius-control)]',
        shape === 'circle' && 'rounded-full',
        className,
      )}
      {...rest}
    />
  );
});
