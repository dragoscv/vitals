import { DropdownMenu as Radix } from 'radix-ui';
import { forwardRef, type ComponentPropsWithoutRef, type ElementRef } from 'react';
import { cn } from '../lib/cn';
import {
  menuContent,
  menuIndicatorSlot,
  menuItem,
  menuItemDanger,
  menuItemInset,
  menuLabel,
  menuSeparator,
  menuShortcut,
} from '../lib/menuStyles';
import { CheckGlyph, ChevronRightGlyph, DotGlyph } from './menuGlyphs';

export const DropdownMenu = Radix.Root;
export const DropdownMenuTrigger = Radix.Trigger;
export const DropdownMenuGroup = Radix.Group;
export const DropdownMenuRadioGroup = Radix.RadioGroup;
export const DropdownMenuSub = Radix.Sub;

/**
 * A menu opened from a button.
 *
 * Radix provides the roving-tabindex keyboard model that a menu is required to
 * have: arrow keys move between items rather than Tab, Home/End jump to the
 * ends, typing a letter jumps to the matching item, Escape closes and returns
 * focus to the trigger, and the trigger's `aria-expanded` stays in sync. That
 * is roughly 400 lines of fiddly, spec-defined behaviour with no product value
 * in re-deriving.
 */
export const DropdownMenuContent = forwardRef<
  ElementRef<typeof Radix.Content>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Content>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuContent({ className, sideOffset = 4, ...rest }, ref) {
  return (
    <Radix.Portal>
      <Radix.Content
        ref={ref}
        sideOffset={sideOffset}
        collisionPadding={8}
        className={cn(menuContent, className)}
        {...rest}
      />
    </Radix.Portal>
  );
});

export interface DropdownMenuItemProps extends Omit<
  ComponentPropsWithoutRef<typeof Radix.Item>,
  'className'
> {
  readonly className?: string;
  /** Marks an irreversible action, such as terminating a process tree. */
  readonly destructive?: boolean;
  /** Aligns with checkable siblings when this item has no indicator. */
  readonly inset?: boolean;
}

export const DropdownMenuItem = forwardRef<ElementRef<typeof Radix.Item>, DropdownMenuItemProps>(
  function DropdownMenuItem({ className, destructive = false, inset = false, ...rest }, ref) {
    return (
      <Radix.Item
        ref={ref}
        className={cn(menuItem, destructive && menuItemDanger, inset && menuItemInset, className)}
        {...rest}
      />
    );
  },
);

export const DropdownMenuCheckboxItem = forwardRef<
  ElementRef<typeof Radix.CheckboxItem>,
  Omit<ComponentPropsWithoutRef<typeof Radix.CheckboxItem>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuCheckboxItem({ className, children, ...rest }, ref) {
  return (
    <Radix.CheckboxItem ref={ref} className={cn(menuItem, menuItemInset, className)} {...rest}>
      <span className={menuIndicatorSlot}>
        <Radix.ItemIndicator>
          <CheckGlyph />
        </Radix.ItemIndicator>
      </span>
      {children}
    </Radix.CheckboxItem>
  );
});

export const DropdownMenuRadioItem = forwardRef<
  ElementRef<typeof Radix.RadioItem>,
  Omit<ComponentPropsWithoutRef<typeof Radix.RadioItem>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuRadioItem({ className, children, ...rest }, ref) {
  return (
    <Radix.RadioItem ref={ref} className={cn(menuItem, menuItemInset, className)} {...rest}>
      <span className={menuIndicatorSlot}>
        <Radix.ItemIndicator>
          <DotGlyph />
        </Radix.ItemIndicator>
      </span>
      {children}
    </Radix.RadioItem>
  );
});

export const DropdownMenuLabel = forwardRef<
  ElementRef<typeof Radix.Label>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Label>, 'className'> & { readonly className?: string }
>(function DropdownMenuLabel({ className, ...rest }, ref) {
  return <Radix.Label ref={ref} className={cn(menuLabel, className)} {...rest} />;
});

export const DropdownMenuSeparator = forwardRef<
  ElementRef<typeof Radix.Separator>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Separator>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuSeparator({ className, ...rest }, ref) {
  return <Radix.Separator ref={ref} className={cn(menuSeparator, className)} {...rest} />;
});

export const DropdownMenuSubTrigger = forwardRef<
  ElementRef<typeof Radix.SubTrigger>,
  Omit<ComponentPropsWithoutRef<typeof Radix.SubTrigger>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuSubTrigger({ className, children, ...rest }, ref) {
  return (
    <Radix.SubTrigger ref={ref} className={cn(menuItem, className)} {...rest}>
      {children}
      <span className="ml-auto pl-4" aria-hidden="true">
        <ChevronRightGlyph />
      </span>
    </Radix.SubTrigger>
  );
});

export const DropdownMenuSubContent = forwardRef<
  ElementRef<typeof Radix.SubContent>,
  Omit<ComponentPropsWithoutRef<typeof Radix.SubContent>, 'className'> & {
    readonly className?: string;
  }
>(function DropdownMenuSubContent({ className, ...rest }, ref) {
  return (
    <Radix.Portal>
      <Radix.SubContent ref={ref} className={cn(menuContent, className)} {...rest} />
    </Radix.Portal>
  );
});

/**
 * A keyboard shortcut hint.
 *
 * `aria-hidden` because Radix already announces the item's own label, and a
 * screen reader reading "Ctrl plus K" as part of the item name is noise. The
 * shortcut still works; it just is not spoken twice.
 */
export function DropdownMenuShortcut({
  className,
  children,
}: {
  readonly className?: string;
  readonly children: React.ReactNode;
}) {
  return (
    <span aria-hidden="true" className={cn(menuShortcut, className)}>
      {children}
    </span>
  );
}
