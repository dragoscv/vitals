import { ContextMenu as Radix } from 'radix-ui';
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
} from '../lib/menuStyles';
import { CheckGlyph, ChevronRightGlyph, DotGlyph } from './menuGlyphs';

export const ContextMenu = Radix.Root;
export const ContextMenuGroup = Radix.Group;
export const ContextMenuRadioGroup = Radix.RadioGroup;
export const ContextMenuSub = Radix.Sub;

/**
 * The right-clickable area.
 *
 * Radix also opens this menu on the **Shift+F10 / Menu key**, which is the
 * only way a keyboard-only user can reach a row's actions. That single detail
 * is the reason this is a Radix primitive rather than an `onContextMenu`
 * handler and an absolutely positioned div: the naive version silently
 * excludes anyone who cannot use a mouse, and in a task manager that means
 * they cannot kill a runaway process.
 */
export const ContextMenuTrigger = Radix.Trigger;

export const ContextMenuContent = forwardRef<
  ElementRef<typeof Radix.Content>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Content>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuContent({ className, ...rest }, ref) {
  return (
    <Radix.Portal>
      <Radix.Content
        ref={ref}
        collisionPadding={8}
        className={cn(menuContent, className)}
        {...rest}
      />
    </Radix.Portal>
  );
});

export interface ContextMenuItemProps extends Omit<
  ComponentPropsWithoutRef<typeof Radix.Item>,
  'className'
> {
  readonly className?: string;
  readonly destructive?: boolean;
  readonly inset?: boolean;
}

export const ContextMenuItem = forwardRef<ElementRef<typeof Radix.Item>, ContextMenuItemProps>(
  function ContextMenuItem({ className, destructive = false, inset = false, ...rest }, ref) {
    return (
      <Radix.Item
        ref={ref}
        className={cn(menuItem, destructive && menuItemDanger, inset && menuItemInset, className)}
        {...rest}
      />
    );
  },
);

export const ContextMenuCheckboxItem = forwardRef<
  ElementRef<typeof Radix.CheckboxItem>,
  Omit<ComponentPropsWithoutRef<typeof Radix.CheckboxItem>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuCheckboxItem({ className, children, ...rest }, ref) {
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

export const ContextMenuRadioItem = forwardRef<
  ElementRef<typeof Radix.RadioItem>,
  Omit<ComponentPropsWithoutRef<typeof Radix.RadioItem>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuRadioItem({ className, children, ...rest }, ref) {
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

export const ContextMenuLabel = forwardRef<
  ElementRef<typeof Radix.Label>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Label>, 'className'> & { readonly className?: string }
>(function ContextMenuLabel({ className, ...rest }, ref) {
  return <Radix.Label ref={ref} className={cn(menuLabel, className)} {...rest} />;
});

export const ContextMenuSeparator = forwardRef<
  ElementRef<typeof Radix.Separator>,
  Omit<ComponentPropsWithoutRef<typeof Radix.Separator>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuSeparator({ className, ...rest }, ref) {
  return <Radix.Separator ref={ref} className={cn(menuSeparator, className)} {...rest} />;
});

export const ContextMenuSubTrigger = forwardRef<
  ElementRef<typeof Radix.SubTrigger>,
  Omit<ComponentPropsWithoutRef<typeof Radix.SubTrigger>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuSubTrigger({ className, children, ...rest }, ref) {
  return (
    <Radix.SubTrigger ref={ref} className={cn(menuItem, className)} {...rest}>
      {children}
      <span className="ml-auto pl-4" aria-hidden="true">
        <ChevronRightGlyph />
      </span>
    </Radix.SubTrigger>
  );
});

export const ContextMenuSubContent = forwardRef<
  ElementRef<typeof Radix.SubContent>,
  Omit<ComponentPropsWithoutRef<typeof Radix.SubContent>, 'className'> & {
    readonly className?: string;
  }
>(function ContextMenuSubContent({ className, ...rest }, ref) {
  return (
    <Radix.Portal>
      <Radix.SubContent ref={ref} className={cn(menuContent, className)} {...rest} />
    </Radix.Portal>
  );
});
