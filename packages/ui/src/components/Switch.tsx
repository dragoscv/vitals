import { Switch as Radix } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef, type ReactNode } from 'react';
import { cn } from '../lib/cn';
import { disabledData, focusRing } from '../lib/styles';
import { useId } from 'react';

export interface SwitchProps extends Omit<
  ComponentPropsWithoutRef<typeof Radix.Root>,
  'className' | 'children'
> {
  readonly className?: string;
  /**
   * Visible label. When present the whole row is clickable and the switch is
   * wired to it with `htmlFor`, which also enlarges the hit target well past
   * the WCAG 2.2 24×24 minimum — the switch itself is only 16px tall.
   */
  readonly label?: ReactNode;
  readonly description?: ReactNode;
  /** Accessible name when no visible `label` is rendered. */
  readonly ariaLabel?: string;
}

/**
 * An immediate on/off control.
 *
 * A switch, not a checkbox, because the setting applies the moment it is
 * toggled — there is no Save button in this app's settings. Screen readers
 * announce the two differently ("on/off" versus "checked"), and the mismatch
 * misleads a user into looking for a confirm step that does not exist.
 */
export const Switch = forwardRef<ElementRef<typeof Radix.Root>, SwitchProps>(function Switch(
  { className, label, description, ariaLabel, id, disabled, ...rest },
  ref,
) {
  const generatedId = useId();
  const switchId = id ?? generatedId;
  const descriptionId = `${switchId}-description`;

  const control = (
    <Radix.Root
      ref={ref}
      id={switchId}
      disabled={disabled}
      aria-label={label === undefined ? ariaLabel : undefined}
      aria-describedby={description === undefined ? undefined : descriptionId}
      className={cn(
        'peer inline-flex h-4.5 w-8 shrink-0 cursor-pointer items-center rounded-full border border-transparent p-0.5',
        'transition-colors duration-(--duration-fast) ease-(--ease-out-quart)',
        'bg-[var(--color-border-strong)] data-[state=checked]:bg-[var(--color-accent)]',
        focusRing,
        disabledData,
        label === undefined ? className : undefined,
      )}
      {...rest}
    >
      <Radix.Thumb
        className={cn(
          'block size-3.5 rounded-full bg-[var(--color-bg-raised)] shadow-sm',
          'transition-transform duration-(--duration-fast) ease-(--ease-out-quart)',
          'translate-x-0 data-[state=checked]:translate-x-3.5',
        )}
      />
    </Radix.Root>
  );

  if (label === undefined) return control;

  return (
    <div className={cn('flex items-start justify-between gap-3', className)}>
      <div className="min-w-0">
        <label
          htmlFor={switchId}
          className={cn(
            'block cursor-pointer text-sm text-[var(--color-fg-default)]',
            disabled === true && 'cursor-not-allowed opacity-50',
          )}
        >
          {label}
        </label>
        {description !== undefined && (
          <p id={descriptionId} className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
            {description}
          </p>
        )}
      </div>
      {control}
    </div>
  );
});
