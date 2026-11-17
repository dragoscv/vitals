import { Tabs as Radix } from 'radix-ui';
import {
  forwardRef,
  useCallback,
  type ComponentPropsWithoutRef,
  type CSSProperties,
  type ElementRef,
} from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';
import { useSlidingIndicator, type SlidingIndicatorGeometry } from '../lib/useSlidingIndicator';

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
 *
 * The selection marker is one element for the whole strip rather than a border
 * on each tab, so the active cue slides from the tab you left to the tab you
 * chose and the eye can follow it. See `TabIndicator`.
 */
export const TabsList = forwardRef<
  ElementRef<typeof Radix.List>,
  Omit<ComponentPropsWithoutRef<typeof Radix.List>, 'className'> & { readonly className?: string }
>(function TabsList({ className, children, ...rest }, ref) {
  // Destructured rather than kept as one object: a fresh object every render
  // would make `setRefs` a new function every render, so React would detach and
  // reattach the list ref (and with it the measurement) on every commit.
  const { ref: measureRef, geometry } = useSlidingIndicator({
    items: '[role="tab"]',
    selected: '[role="tab"][data-state="active"]',
  });

  // The Radix list needs the node for its own focus bookkeeping and so does the
  // measurement, so the two refs are composed rather than one replacing the
  // other — dropping either one breaks a different thing silently.
  const setRefs = useCallback(
    (element: HTMLDivElement | null) => {
      measureRef(element);
      if (typeof ref === 'function') ref(element);
      else if (ref !== null) ref.current = element;
    },
    [measureRef, ref],
  );

  return (
    <Radix.List
      ref={setRefs}
      className={cn(
        // `relative` is what makes the indicator's inline offsets resolve
        // against the strip rather than the nearest positioned ancestor.
        'relative flex items-center gap-1 border-b border-[var(--color-border-subtle)]',
        className,
      )}
      {...rest}
    >
      {children}
      {geometry !== null && <TabIndicator geometry={geometry} />}
    </Radix.List>
  );
});

/**
 * The marker that slides between tabs.
 *
 * Absolutely positioned and moved with a transform, so the slide is a
 * compositor job. `aria-hidden` because the selection is already exposed by
 * `aria-selected` on the trigger — announcing the same state twice is noise.
 *
 * Orientation decides the shape: a bottom bar in a row, a left bar in a column.
 * A bottom border in a vertical strip floats under a label with nothing beside
 * it, which is what the per-tab border it replaces did.
 */
function TabIndicator({ geometry }: { readonly geometry: SlidingIndicatorGeometry }) {
  const style: CSSProperties =
    geometry.orientation === 'horizontal'
      ? {
          height: 2,
          width: geometry.width,
          left: 0,
          // -1px because the strip's own 1px border sits below its padding box;
          // the bar is drawn 1px lower so the two 2px edges coincide, the way
          // the per-tab border used to.
          bottom: -1,
          transform: `translate3d(${geometry.left}px, 0, 0)`,
        }
      : {
          width: 2,
          height: geometry.height,
          left: 0,
          top: 0,
          transform: `translate3d(0, ${geometry.top}px, 0)`,
        };

  return (
    <span
      aria-hidden="true"
      data-tab-indicator=""
      className="vitals-tab-indicator pointer-events-none absolute rounded-full bg-[var(--color-accent)]"
      style={style}
    />
  );
}

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
        // Colour is not the only cue: the active tab gains bolder weight and
        // the sliding marker. The border is left to the indicator, so the cue
        // is not drawn twice — once instantly on the new tab, once still
        // sliding towards it.
        'data-[state=active]:font-semibold data-[state=active]:text-[var(--color-fg-default)]',
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
