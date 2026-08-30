import { Separator as RadixSeparator } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef } from 'react';
import { cn } from '../lib/cn';

export interface SeparatorProps extends Omit<
  ComponentPropsWithoutRef<typeof RadixSeparator.Root>,
  'className' | 'decorative'
> {
  readonly className?: string;
  /**
   * Whether the rule carries meaning.
   *
   * Defaults to decorative, which renders `role="none"`. Most separators are
   * visual grouping only, and announcing every one of them as a separator
   * turns a dense toolbar into a stream of "separator, separator, separator".
   * Set to false where the divide is genuinely semantic, such as between a
   * menu's destructive actions and the rest.
   */
  readonly decorative?: boolean;
}

export const Separator = forwardRef<ElementRef<typeof RadixSeparator.Root>, SeparatorProps>(
  function Separator({ className, orientation = 'horizontal', decorative = true, ...rest }, ref) {
    return (
      <RadixSeparator.Root
        ref={ref}
        orientation={orientation}
        decorative={decorative}
        className={cn(
          'shrink-0 bg-[var(--color-border-subtle)]',
          orientation === 'horizontal' ? 'h-px w-full' : 'h-full w-px',
          className,
        )}
        {...rest}
      />
    );
  },
);
