import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { TooltipProvider } from '@vitals/ui';

import { toggleHud } from '../lib/settingsSync';
import { RouteView } from '../routes';
import { useSettings } from '../settings/store';
import { Content } from './Content';
import { ErrorBoundary } from './ErrorBoundary';
import { navItems } from './navigation';
import { RouteError } from './RouteError';
import { Sidebar } from './Sidebar';
import type { ShortcutActions } from './shortcuts';
import { TitleBar } from './TitleBar';
import { useSamplerToasts } from './useSamplerToasts';
import { useShortcuts } from './useShortcuts';

// Lazy: the dialog and its panels are 17 KB of the entry chunk for a surface
// most sessions never open. It renders nothing until `open`, so there is no
// fallback to show — the chunk loads in the moment between the click and the
// dialog's own open animation.
const SettingsDialog = lazy(async () => ({
  default: (await import('../settings/SettingsDialog')).SettingsDialog,
}));

// Same reasoning: the palette and the shortcut sheet are keyboard-only
// surfaces that most sessions never open, and both render nothing while
// closed. Deferring them keeps Motion's and their own weight out of the first
// paint.
// sonner is 9.7 KB gzipped and a toast is a rare event; the region mounts
// after first paint. Anything fired before it lands is queued by sonner's
// own store, so nothing is lost.
const Toaster = lazy(async () => ({
  default: (await import('@vitals/ui/toast')).Toaster,
}));

const CommandPalette = lazy(async () => ({
  default: (await import('./CommandPalette')).CommandPalette,
}));
const ShortcutsHelp = lazy(async () => ({
  default: (await import('./ShortcutsHelp')).ShortcutsHelp,
}));

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
  const patch = useSettings((state) => state.patch);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
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

  /**
   * Flips the overlay from the palette.
   *
   * Goes through the same backend round trip as the Ctrl+Shift+H binding in
   * `useHud` and writes back what the backend reports, so the settings switch
   * cannot disagree with the window that is actually on screen.
   */
  const onToggleHud = useCallback(() => {
    void toggleHud().then((actual) => {
      if (actual !== null) patch({ hudVisible: actual });
    });
  }, [patch]);

  const shortcutActions = useMemo<ShortcutActions>(
    () => ({
      openPalette: () => setPaletteOpen(true),
      openHelp: () => setHelpOpen(true),
      openSettings: () => onSettingsOpenChange(true),
      navigate,
    }),
    [navigate, onSettingsOpenChange],
  );
  useShortcuts(shortcutActions);
  useSamplerToasts();

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

      {/* Only mounted once it has been opened, so the chunk is never fetched
          for a session that never touches settings. `null` is the right
          fallback: a closed dialog renders nothing anyway. */}
      {settingsOpen && (
        <Suspense fallback={null}>
          <SettingsDialog
            open={settingsOpen}
            onOpenChange={onSettingsOpenChange}
            version={version}
          />
        </Suspense>
      )}

      {paletteOpen && (
        <Suspense fallback={null}>
          <CommandPalette
            open={paletteOpen}
            onOpenChange={setPaletteOpen}
            onNavigate={navigate}
            onOpenSettings={() => onSettingsOpenChange(true)}
            onToggleHud={onToggleHud}
          />
        </Suspense>
      )}

      {helpOpen && (
        <Suspense fallback={null}>
          <ShortcutsHelp open={helpOpen} onOpenChange={setHelpOpen} />
        </Suspense>
      )}

      <Suspense fallback={null}>
        <Toaster
          regionLabel={t('common.notifications')}
          closeLabel={t('common.dismissNotification')}
        />
      </Suspense>
    </TooltipProvider>
  );
}
