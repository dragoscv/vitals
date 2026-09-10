import { ToggleGroup as Radix } from 'radix-ui';
import { forwardRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';

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
  return (
    <Radix.Root
      ref={ref}
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
        'inline-flex items-center gap-0.5 rounded-[var(--radius-control)] bg-[var(--color-bg-inset)] p-0.5',
        className,
      )}
    >
      {options.map((option) => (
        <Radix.Item
          key={option.value}
          value={option.value}
          disabled={option.disabled}
          aria-label={option.ariaLabel}
          className={cn(
            'inline-flex items-center justify-center gap-1.5 rounded-[calc(var(--radius-control)-2px)] font-medium whitespace-nowrap text-[var(--color-fg-muted)]',
            'transition-colors duration-(--duration-fast)',
            size === 'sm' ? 'h-6 px-2 text-2xs' : 'h-7 px-2.5 text-2xs',
            'hover:text-[var(--color-fg-default)]',
            'data-[state=on]:bg-[var(--color-bg-raised)] data-[state=on]:text-[var(--color-fg-default)] data-[state=on]:shadow-[var(--shadow-widget)]',
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
