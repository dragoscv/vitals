import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { TooltipProvider } from '@vitals/ui';

import { RouteView } from '../routes';
import { SettingsDialog } from '../settings/SettingsDialog';
import { useSettings } from '../settings/store';
import { Content } from './Content';
import { ErrorBoundary } from './ErrorBoundary';
import { navItems } from './navigation';
import { RouteError } from './RouteError';
import { Sidebar } from './Sidebar';
import { TitleBar } from './TitleBar';

/**
 * Below this width the sidebar is forced to icons only.
 *
 * 900px, not the 720px window minimum: at 720 an expanded 224px sidebar leaves
 * under 500px of content, which is narrower than a single dashboard card. The
 * user's own collapsed preference is still stored untouched, so widening the
 * window restores exactly what they chose — a forced collapse is a temporary
 * consequence of the viewport, never a change to their setting.
 */
const FORCE_COLLAPSE_BELOW = 900;

function useNarrowViewport(): boolean {
  const [narrow, setNarrow] = useState(
    () => globalThis.matchMedia?.(`(max-width: ${FORCE_COLLAPSE_BELOW - 1}px)`).matches ?? false,
  );

  useEffect(() => {
    const media = globalThis.matchMedia?.(`(max-width: ${FORCE_COLLAPSE_BELOW - 1}px)`);
    if (!media) return;
    const onChange = (event: MediaQueryListEvent) => setNarrow(event.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, []);

  return narrow;
}

export interface AppShellProps {
  readonly version: string;
}

export function AppShell({ version }: AppShellProps) {
  const { t } = useTranslation();
  const route = useSettings((state) => state.route);
  const navigate = useSettings((state) => state.navigate);
  const collapsedSetting = useSettings((state) => state.settings.sidebarCollapsed);
  const toggleSidebar = useSettings((state) => state.toggleSidebar);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settingsTrigger = useRef<HTMLElement | null>(null);

  /**
   * The dialog is opened from state rather than a `DialogTrigger`, so Radix
   * has no trigger to hand focus back to. Without this, dismissing settings
   * drops a keyboard user at the top of the document and they must tab
   * through the whole sidebar to get back to where they were.
   */
  const onSettingsOpenChange = useCallback((open: boolean) => {
    if (open) {
      settingsTrigger.current = document.activeElement as HTMLElement | null;
    } else {
      const trigger = settingsTrigger.current;
      settingsTrigger.current = null;
      // Deferred a task, because Radix moves focus itself as the overlay
      // unmounts. Focusing synchronously here wins the race and then loses it
      // a tick later, leaving focus on the body — the exact failure this is
      // meant to prevent.
      setTimeout(() => trigger?.focus(), 0);
    }
    setSettingsOpen(open);
  }, []);

  const narrow = useNarrowViewport();
  const collapsed = collapsedSetting || narrow;

  const activeItem = navItems.find((item) => item.id === route);
  const sectionName = activeItem ? t(activeItem.labelKey) : t('app.name');

  // The document title tracks the section so the taskbar preview and any
  // window-switcher say where you are, not just which app you are in.
  useEffect(() => {
    document.title = `${sectionName} — ${t('app.name')}`;
  }, [sectionName, t]);

  return (
    <TooltipProvider>
      <div className="flex h-full flex-col">
        <TitleBar>{sectionName}</TitleBar>

        <div className="flex min-h-0 flex-1">
          <Sidebar
            active={route}
            onNavigate={navigate}
            collapsed={collapsed}
            // Toggling while force-collapsed would write a preference the user
            // cannot see the effect of, so the control is a no-op there and the
            // stored value keeps meaning what they last chose deliberately.
            onToggleCollapsed={narrow ? () => {} : toggleSidebar}
            onOpenSettings={() => onSettingsOpenChange(true)}
          />

          <Content routeKey={route}>
            {/*
             * The boundary is INSIDE Content, not around the whole shell: a
             * crash in one screen must leave the sidebar and Settings usable.
             * A monitoring tool that blanks its own window when something goes
             * wrong looks like it broke the machine it was meant to diagnose.
             *
             * `resetKey={route}` clears a stale error on navigation — without
             * it, one screen crashing would show its error over every screen
             * the user visited afterwards.
             */}
            <ErrorBoundary
              resetKey={route}
              fallback={(error, retry) => <RouteError error={error} onRetry={retry} />}
            >
              {/* `navigate` is threaded through so a screen can send the user
                  elsewhere — the dashboard's alerts link to the section that
                  explains them, which is what makes them actionable. */}
              <RouteView route={route} onNavigate={navigate} />
            </ErrorBoundary>
          </Content>
        </div>
      </div>

      <SettingsDialog open={settingsOpen} onOpenChange={onSettingsOpenChange} version={version} />
    </TooltipProvider>
  );
}
