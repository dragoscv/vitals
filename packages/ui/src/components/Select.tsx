import { Select as Radix } from 'radix-ui';
import { forwardRef, useId, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing, overlayMotion, overlaySurface } from '../lib/styles';
import { CheckGlyph, ChevronDownGlyph } from './menuGlyphs';

export interface SelectOption {
  readonly value: string;
  readonly label: string;
  readonly disabled?: boolean;
}

export interface SelectGroup {
  readonly label: string;
  readonly options: readonly SelectOption[];
}

export interface SelectProps {
  readonly className?: string;
  readonly value?: string;
  readonly defaultValue?: string;
  readonly onValueChange?: (value: string) => void;
  /**
   * A flat option list or grouped sections.
   *
   * Data-driven rather than a compound `<Select.Item>` API. Nearly every select
   * in this app is populated from a runtime list (CPU cores, network adapters,
   * update channels) and would otherwise be a `.map()` at every call site; the
   * grouped form is a discriminated shape so a group and an option can never be
   * confused.
   */
  readonly options: readonly SelectOption[] | readonly SelectGroup[];
  /** Shown when nothing is selected. Caller-supplied text. */
  readonly placeholder?: string;
  readonly label?: ReactNode;
  readonly ariaLabel?: string;
  readonly disabled?: boolean;
  readonly name?: string;
  readonly id?: string;
}

function isGrouped(
  options: readonly SelectOption[] | readonly SelectGroup[],
): options is readonly SelectGroup[] {
  const first = options[0];
  return first !== undefined && 'options' in first;
}

const itemClass = cn(
  'relative flex cursor-default items-center rounded-[var(--radius-control)] py-1.5 pr-2 pl-7 text-2xs outline-none select-none',
  'data-highlighted:bg-[var(--color-accent-subtle)] data-highlighted:text-[var(--color-accent)]',
  disabledData,
);

function renderOption(option: SelectOption) {
  return (
    // Spread rather than `disabled={option.disabled}`: Radix declares
    // `disabled: boolean`, and under `exactOptionalPropertyTypes` an explicit
    // `undefined` is not the same as an absent prop.
    <Radix.Item
      key={option.value}
      value={option.value}
      {...(option.disabled !== undefined && { disabled: option.disabled })}
      className={itemClass}
    >
      <span className="absolute left-2 inline-flex size-3.5 items-center justify-center">
        <Radix.ItemIndicator>
          <CheckGlyph className="size-3" />
        </Radix.ItemIndicator>
      </span>
      <Radix.ItemText>{option.label}</Radix.ItemText>
    </Radix.Item>
  );
}

/**
 * A single-choice dropdown.
 *
 * A Radix listbox rather than a native `<select>`: the native element cannot be
 * styled to match the rest of this UI on Windows, and its popup is rendered by
 * the OS outside the WebView, so it ignores the theme entirely and looks like a
 * different application. Radix keeps the parts that matter — typeahead, arrow
 * key navigation, Home/End, Escape to dismiss, and focus returning to the
 * trigger on close.
 */
export const Select = forwardRef<HTMLButtonElement, SelectProps>(function Select(
  {
    className,
    value,
    defaultValue,
    onValueChange,
    options,
    placeholder,
    label,
    ariaLabel,
    disabled,
    name,
    id,
  },
  ref,
) {
  const generatedId = useId();
  const triggerId = id ?? generatedId;

  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {label !== undefined && (
        <label htmlFor={triggerId} className="text-2xs font-medium text-[var(--color-fg-muted)]">
          {label}
        </label>
      )}
      <Radix.Root
        {...(value !== undefined && { value })}
        {...(defaultValue !== undefined && { defaultValue })}
        {...(onValueChange !== undefined && { onValueChange })}
        {...(name !== undefined && { name })}
        {...(disabled !== undefined && { disabled })}
      >
        <Radix.Trigger
          ref={ref}
          id={triggerId}
          aria-label={label === undefined ? ariaLabel : undefined}
          className={cn(
            'inline-flex h-8 w-full items-center justify-between gap-2 rounded-[var(--radius-control)] border border-[var(--color-border-default)] bg-[var(--color-bg-inset)] px-2 text-sm text-[var(--color-fg-default)]',
            'data-placeholder:text-[var(--color-fg-subtle)]',
            focusRing,
            disabledData,
          )}
        >
          <Radix.Value placeholder={placeholder} />
          <Radix.Icon asChild>
            <span className="shrink-0 text-[var(--color-fg-subtle)]">
              <ChevronDownGlyph />
            </span>
          </Radix.Icon>
        </Radix.Trigger>

        <Radix.Portal>
          <Radix.Content
            position="popper"
            sideOffset={4}
            collisionPadding={8}
            className={cn(
              overlaySurface,
              overlayMotion,
              'max-h-72 min-w-(--radix-select-trigger-width)',
            )}
          >
            <Radix.Viewport className="p-1">
              {isGrouped(options)
                ? options.map((group) => (
                    <Radix.Group key={group.label}>
                      <Radix.Label className="px-2 py-1 text-2xs font-semibold text-[var(--color-fg-subtle)]">
                        {group.label}
                      </Radix.Label>
                      {group.options.map(renderOption)}
                    </Radix.Group>
                  ))
                : options.map(renderOption)}
            </Radix.Viewport>
          </Radix.Content>
        </Radix.Portal>
      </Radix.Root>
    </div>
  );
});
