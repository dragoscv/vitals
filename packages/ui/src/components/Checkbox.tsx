import { Checkbox as Radix } from 'radix-ui';
import {
  forwardRef,
  useId,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type ReactNode,
} from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';
import { CheckGlyph, MinusGlyph } from './menuGlyphs';

export interface CheckboxProps extends Omit<
  ComponentPropsWithoutRef<typeof Radix.Root>,
  'className' | 'children'
> {
  readonly className?: string;
  readonly label?: ReactNode;
  /** Accessible name when no visible `label` is rendered — e.g. a row selector. */
  readonly ariaLabel?: string;
}

/**
 * A tri-state checkbox.
 *
 * The indeterminate state is a real value (`checked="indeterminate"`), not a
 * DOM property poked in after render. A "select all" header box over a
 * partially selected process list *must* report `aria-checked="mixed"`; a
 * plain unchecked box tells a screen reader user that nothing is selected
 * while rows visibly are.
 */
export const Checkbox = forwardRef<ElementRef<typeof Radix.Root>, CheckboxProps>(function Checkbox(
  { className, label, ariaLabel, id, disabled, ...rest },
  ref,
) {
  const generatedId = useId();
  const boxId = id ?? generatedId;

  const control = (
    <Radix.Root
      ref={ref}
      id={boxId}
      disabled={disabled}
      aria-label={label === undefined ? ariaLabel : undefined}
      className={cn(
        'inline-flex size-4 shrink-0 items-center justify-center rounded-[4px] border border-[var(--color-border-strong)] bg-[var(--color-bg-raised)]',
        'transition-colors duration-(--duration-fast)',
        'data-[state=checked]:border-[var(--color-accent)] data-[state=checked]:bg-[var(--color-accent)] data-[state=checked]:text-[var(--color-fg-on-accent)]',
        'data-[state=indeterminate]:border-[var(--color-accent)] data-[state=indeterminate]:bg-[var(--color-accent)] data-[state=indeterminate]:text-[var(--color-fg-on-accent)]',
        focusRing,
        disabledData,
        label === undefined ? className : undefined,
      )}
      {...rest}
    >
      <Radix.Indicator className="flex items-center justify-center">
        {rest.checked === 'indeterminate' ? (
          <MinusGlyph className="size-3" />
        ) : (
          <CheckGlyph className="size-3" />
        )}
      </Radix.Indicator>
    </Radix.Root>
  );

  if (label === undefined) return control;

  return (
    // The label is a sibling wired by `htmlFor` rather than a wrapper: nesting
    // a Radix checkbox inside a <label> makes a click on the box bubble to the
    // label, which re-fires the toggle and cancels it out.
    <div className={cn('flex items-center gap-2', className)}>
      {control}
      <label
        htmlFor={boxId}
        className={cn(
          'cursor-pointer text-sm text-[var(--color-fg-default)]',
          disabled === true && 'cursor-not-allowed opacity-50',
        )}
      >
        {label}
      </label>
    </div>
  );
});
