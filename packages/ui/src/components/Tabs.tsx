import { Tabs as Radix } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef } from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';

export const Tabs = Radix.Root;

/**
 * The tab strip.
 *
 * Radix implements the WAI-ARIA tabs pattern: exactly one tab is in the tab
 * order and arrow keys move between them. Rendering plain buttons instead
 * forces a keyboard user to Tab through every tab to reach the panel, which in
 * a nine-tab monitor is eight unnecessary stops on the way to the content.
 *
 * Activation is manual (`activationMode="manual"`) rather than automatic:
 * arrowing across tabs that each start a sampler would spin up and tear down
 * a subscription per keypress. The user presses Enter or Space to commit.
 */
export const TabsList = forwardRef<
  ElementRef<typeof Radix.List>,
  Omit<ComponentPropsWithoutRef<typeof Radix.List>, 'className'> & { readonly className?: string }
>(function TabsList({ className, ...rest }, ref) {
  return (
    <Radix.List
      ref={ref}
      className={cn(
        'flex items-center gap-1 border-b border-[var(--color-border-subtle)]',
        className,
      )}
      {...rest}
    />
  );
});

export const TabsTrigger = forwardRef<
  ElementRef<typeof Radix.Trigger>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Trigger>, 'className'> & {
    readonly className?: string;
  }
>(function TabsTrigger({ className, ...rest }, ref) {
  return (
    <Radix.Trigger
      ref={ref}
      className={cn(
        'relative -mb-px inline-flex h-8 items-center gap-1.5 border-b-2 border-transparent px-3 text-2xs font-medium whitespace-nowrap text-[var(--color-fg-muted)]',
        'transition-colors duration-(--duration-fast)',
        'hover:text-[var(--color-fg-default)]',
        // Colour is not the only cue: the active tab also gains a bottom
        // border and bolder weight, so the selection survives greyscale and
        // any accent the user picks.
        'data-[state=active]:border-[var(--color-accent)] data-[state=active]:font-semibold data-[state=active]:text-[var(--color-fg-default)]',
        focusRing,
        disabledData,
        className,
      )}
      {...rest}
    />
  );
});

export const TabsContent = forwardRef<
  ElementRef<typeof Radix.Content>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Content>, 'className'> & {
    readonly className?: string;
  }
>(function TabsContent({ className, ...rest }, ref) {
  return <Radix.Content ref={ref} className={cn('min-h-0 outline-none', className)} {...rest} />;
});
