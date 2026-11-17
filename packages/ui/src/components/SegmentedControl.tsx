import { ToggleGroup as Radix } from 'radix-ui';
import { forwardRef, useCallback, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';
import { useSlidingIndicator } from '../lib/useSlidingIndicator';

export interface SegmentedOption<T extends string> {
  readonly value: T;
  readonly label: ReactNode;
  /** Required when `label` is an icon, so the segment still has a name. */
  readonly ariaLabel?: string;
  readonly disabled?: boolean;
}

export interface SegmentedControlProps<T extends string> {
  readonly className?: string;
  readonly value: T;
  readonly onValueChange: (value: T) => void;
  readonly options: readonly SegmentedOption<T>[];
  /** Names the group as a whole, e.g. "Time range". */
  readonly ariaLabel: string;
  readonly size?: 'sm' | 'md';
}

/**
 * A compact one-of-many switcher — time ranges, view modes, units.
 *
 * Built on Radix's single-select toggle group, which gives roving focus: one
 * Tab stop for the whole control and arrow keys within it. A row of plain
 * buttons would put six stops in the tab order for what is conceptually one
 * setting.
 *
 * The generic parameter is deliberate. `value: string` would let a typo in a
 * segment value compile and then silently never match, leaving a control where
 * nothing appears selected and nobody can see why.
 */
function SegmentedControlInner<T extends string>(
  { className, value, onValueChange, options, ariaLabel, size = 'md' }: SegmentedControlProps<T>,
  ref: React.Ref<ElementRef<typeof Radix.Root>>,
) {
  const { ref: measureRef, geometry } = useSlidingIndicator({
    items: '[role="radio"]',
    selected: '[role="radio"][data-state="on"]',
  });

  const setRefs = useCallback(
    (element: HTMLDivElement | null) => {
      measureRef(element);
      if (typeof ref === 'function') ref(element);
      else if (ref !== null) ref.current = element;
    },
    [measureRef, ref],
  );

  return (
    <Radix.Root
      ref={setRefs}
      type="single"
      value={value}
      // Radix emits '' when the pressed item is toggled off. A segmented
      // control has no "none" state, so that is swallowed rather than
      // forwarded — otherwise clicking the active segment clears the setting.
      onValueChange={(next) => {
        if (next !== '') onValueChange(next as T);
      }}
      aria-label={ariaLabel}
      className={cn(
        // `relative` so the indicator's offsets resolve against the control.
        'relative inline-flex items-center gap-0.5 rounded-[var(--radius-control)] bg-[var(--color-bg-inset)] p-0.5',
        className,
      )}
    >
      {geometry !== null && (
        <span
          aria-hidden="true"
          data-segment-indicator=""
          // Rendered before the segments and with no z-index, so the segments
          // — which are `relative` — paint their labels over it. An absolutely
          // positioned element otherwise wins against in-flow content and the
          // pill would cover the text it is selecting.
          className="vitals-tab-indicator pointer-events-none absolute top-0 left-0 rounded-[calc(var(--radius-control)-2px)] bg-[var(--color-bg-raised)] shadow-[var(--shadow-widget)]"
          style={{
            width: geometry.width,
            height: geometry.height,
            transform: `translate3d(${geometry.left}px, ${geometry.top}px, 0)`,
          }}
        />
      )}
      {options.map((option) => (
        <Radix.Item
          key={option.value}
          value={option.value}
          disabled={option.disabled}
          aria-label={option.ariaLabel}
          className={cn(
            'relative inline-flex items-center justify-center gap-1.5 rounded-[calc(var(--radius-control)-2px)] font-medium whitespace-nowrap text-[var(--color-fg-muted)]',
            'transition-colors duration-(--duration-fast)',
            size === 'sm' ? 'h-6 px-2 text-2xs' : 'h-7 px-2.5 text-2xs',
            'hover:text-[var(--color-fg-default)]',
            // The raised fill and its shadow moved to the sliding indicator:
            // leaving them on the segment would draw the "on" state twice —
            // once instantly under the new segment, once still travelling.
            'data-[state=on]:text-[var(--color-fg-default)]',
            focusRing,
            disabledData,
          )}
        >
          {option.label}
        </Radix.Item>
      ))}
    </Radix.Root>
  );
}

/**
 * `forwardRef` erases generics, so the cast restores the parameterised
 * signature. Without it every consumer would widen `value` to `string` and
 * lose the exhaustiveness the union exists to provide.
 */
export const SegmentedControl = forwardRef(SegmentedControlInner) as <T extends string>(
  props: SegmentedControlProps<T> & { readonly ref?: React.Ref<ElementRef<typeof Radix.Root>> },
) => React.ReactElement;
