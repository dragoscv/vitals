import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

/**
 * Merges class names, resolving Tailwind conflicts in favour of the last one.
 *
 * Without `twMerge`, `cn('p-2', 'p-4')` emits both classes and the winner
 * depends on stylesheet order rather than call order — which makes a
 * `className` prop unreliable for overriding, and is why component libraries
 * that skip this step end up with `!important` scattered through them.
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
