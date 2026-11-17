import { Collapsible as Radix } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';

export const Collapsible = Radix.Root;
export const CollapsibleTrigger = Radix.Trigger;

export interface CollapsibleContentProps extends Omit<
  ComponentPropsWithoutRef<typeof Radix.Content>,
  'className'
> {
  readonly className?: string;
  readonly children?: ReactNode;
}

/**
 * The collapsible panel.
 *
 * Radix supplies the part that is easy to get wrong: `aria-expanded` on the
 * trigger, `aria-controls` pointing at the panel, and the panel removed from
 * the accessibility tree while closed so its contents are not tabbable. A
 * hand-rolled `hidden` toggle leaves a closed group's rows in the tab order,
 * which is how a keyboard user ends up tabbing into something they cannot see.
 *
 * What this adds is the animation, and the reason it is structural rather than
 * a class on the caller's own div: `grid-template-rows: 0fr -> 1fr` only
 * collapses if the child of the grid clips its own overflow, so an inner
 * wrapper is part of the mechanism, not decoration. The two rules and the
 * keyframes live in `theme.css` under `.vitals-collapsible`.
 */
export const CollapsibleContent = forwardRef<
  ElementRef<typeof Radix.Content>,
  CollapsibleContentProps
>(function CollapsibleContent({ className, children, ...rest }, ref) {
  return (
    <Radix.Content ref={ref} className={cn('vitals-collapsible', className)} {...rest}>
      {/* Carries no class of its own: the clip is `.vitals-collapsible > *` in
          theme.css, so a caller cannot lose it by overriding `className`. */}
      <div>{children}</div>
    </Radix.Content>
  );
});
