import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';
import { TooltipProvider } from '@vitals/ui';

import { navItems } from './navigation';
import { Sidebar } from './Sidebar';
import { registerShellStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

function renderSidebar(overrides: Partial<Parameters<typeof Sidebar>[0]> = {}) {
  const onNavigate = vi.fn();
  const onToggleCollapsed = vi.fn();
  const onOpenSettings = vi.fn();

  const result = render(
    // Mirrors the shell, which mounts one provider near the root so hovering
    // between adjacent icon-only controls does not restart the delay.
    <TooltipProvider>
      <Sidebar
        active="dashboard"
        onNavigate={onNavigate}
        collapsed={false}
        onToggleCollapsed={onToggleCollapsed}
        onOpenSettings={onOpenSettings}
        {...overrides}
      />
    </TooltipProvider>,
  );

  return { ...result, onNavigate, onToggleCollapsed, onOpenSettings };
}

function navButtons(): HTMLButtonElement[] {
  return Array.from(document.querySelectorAll<HTMLButtonElement>('[data-nav-item]'));
}

describe('Sidebar', () => {
  it('names the navigation landmark', () => {
    renderSidebar();
    expect(screen.getByRole('navigation', { name: 'Main navigation' })).toBeTruthy();
  });

  it('renders every destination', () => {
    renderSidebar();
    expect(navButtons()).toHaveLength(navItems.length);
  });

  it('marks only the active destination with aria-current', () => {
    renderSidebar({ active: 'processes' });

    const current = navButtons().filter((button) => button.getAttribute('aria-current') === 'page');
    expect(current).toHaveLength(1);
    expect(current[0]?.dataset['navItem']).toBe('processes');
  });

  it('puts exactly one navigation item in the tab order', () => {
    // The WAI-ARIA pattern: one stop for the group, arrows within it. Twelve
    // tab stops would sit between the title bar and the content on every visit.
    renderSidebar({ active: 'network' });

    const tabbable = navButtons().filter((button) => button.tabIndex === 0);
    expect(tabbable).toHaveLength(1);
    expect(tabbable[0]?.dataset['navItem']).toBe('network');
  });

  it('moves focus with the arrow keys without navigating', () => {
    const { onNavigate } = renderSidebar({ active: 'dashboard' });
    const buttons = navButtons();

    buttons[0]?.focus();
    fireEvent.keyDown(buttons[0] as HTMLElement, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(buttons[1]);

    fireEvent.keyDown(buttons[1] as HTMLElement, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(buttons[2]);

    // Arrowing past a section must not mount it — each one starts a sampler.
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it('wraps at both ends and supports Home and End', () => {
    renderSidebar();
    const buttons = navButtons();
    const last = buttons.length - 1;

    buttons[0]?.focus();
    fireEvent.keyDown(buttons[0] as HTMLElement, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(buttons[last]);

    fireEvent.keyDown(buttons[last] as HTMLElement, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(buttons[0]);

    fireEvent.keyDown(buttons[0] as HTMLElement, { key: 'End' });
    expect(document.activeElement).toBe(buttons[last]);

    fireEvent.keyDown(buttons[last] as HTMLElement, { key: 'Home' });
    expect(document.activeElement).toBe(buttons[0]);
  });

  it('navigates on activation', () => {
    const { onNavigate } = renderSidebar();
    fireEvent.click(screen.getByRole('button', { name: 'Processes' }));
    expect(onNavigate).toHaveBeenCalledWith('processes');
  });

  it('keeps an accessible name for every item when collapsed', () => {
    // Collapsed, the icon is all that renders. Without a name each item is
    // announced as a bare "button" and the whole sidebar becomes unusable.
    renderSidebar({ collapsed: true });

    for (const button of navButtons()) {
      const name = button.textContent?.trim() || button.getAttribute('aria-label');
      expect(name, `${button.dataset['navItem'] ?? '?'} has no accessible name`).toBeTruthy();
    }
  });

  it('offers the collapse control and reports the resulting state', () => {
    const { onToggleCollapsed, rerender } = renderSidebar();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }));
    expect(onToggleCollapsed).toHaveBeenCalledTimes(1);

    rerender(
      <TooltipProvider>
        <Sidebar
          active="dashboard"
          onNavigate={vi.fn()}
          collapsed
          onToggleCollapsed={onToggleCollapsed}
          onOpenSettings={vi.fn()}
        />
      </TooltipProvider>,
    );

    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeTruthy();
  });

  it('opens settings from the footer', () => {
    const { onOpenSettings } = renderSidebar();
    const nav = screen.getByRole('navigation', { name: 'Main navigation' });
    fireEvent.click(within(nav).getByRole('button', { name: 'Settings' }));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });
});
