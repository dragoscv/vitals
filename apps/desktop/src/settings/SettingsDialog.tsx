import {
  Activity,
  Bell,
  HeartPulse,
  Info,
  Palette,
  ShieldCheck,
  SlidersHorizontal,
  Smartphone,
  Timer,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  DialogContent,
  DialogRoot,
  ScrollArea,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  cn,
} from '@vitals/ui';

import { OPEN_SETTINGS_EVENT, takeSettingsTab } from '../shell/navigation';
import { SHELL_NS } from '../shell/strings';
import { HealthPanel } from './health/HealthPanel';
import {
  AboutPanel,
  AppearancePanel,
  GeneralPanel,
  NotificationsPanel,
  PrivacyPanel,
  SamplingPanel,
} from './panels';
import { RemoteAccessPanel } from './lan/RemoteAccessPanel';
import { WatchdogPanel } from './WatchdogPanel';

const tabIds = [
  'general',
  'appearance',
  'sampling',
  'notifications',
  'watchdog',
  'remote',
  'health',
  'privacy',
  'about',
] as const;

type TabId = (typeof tabIds)[number];

const tabIcons: Readonly<Record<TabId, LucideIcon>> = {
  general: SlidersHorizontal,
  appearance: Palette,
  sampling: Timer,
  notifications: Bell,
  watchdog: Activity,
  remote: Smartphone,
  health: HeartPulse,
  privacy: ShieldCheck,
  about: Info,
};

export interface SettingsDialogProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly version: string;
}

export function SettingsDialog({ open, onOpenChange, version }: SettingsDialogProps) {
  const { t } = useTranslation();
  const { t: ts } = useTranslation(SHELL_NS);
  // Kept across open/close on purpose. Someone who closes the dialog to try a
  // theme change and reopens it wants the tab they were on, not General.
  const [tab, setTab] = useState<TabId>(() => requestedTab() ?? 'general');

  // Opened again while already mounted ("Pair again" with the dialog open):
  // honour a tab request that arrives with the event.
  useEffect(() => {
    const onOpen = (): void => {
      const requested = requestedTab();
      if (requested !== null) setTab(requested);
    };
    window.addEventListener(OPEN_SETTINGS_EVENT, onOpen);
    return () => {
      window.removeEventListener(OPEN_SETTINGS_EVENT, onOpen);
    };
  }, []);

  const labels: Readonly<Record<TabId, string>> = {
    general: ts('settings.general.title'),
    appearance: t('settings.appearance.title'),
    sampling: t('settings.sampling.title'),
    notifications: ts('settings.notifications.title'),
    watchdog: ts('settings.watchdog.title'),
    remote: ts('settings.remote.title'),
    health: ts('settings.health.title'),
    privacy: t('settings.privacy.title'),
    about: t('settings.about.title'),
  };

  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="xl"
        title={t('settings.title')}
        closeLabel={t('common.close')}
        // Capped in `vh` as well as `rem`: settings must remain closeable on a
        // 720p laptop, and a dialog taller than the viewport puts its close
        // button off-screen with no way back other than Escape.
        className="h-[min(34rem,calc(100vh-4rem))] max-w-3xl"
      >
        <Tabs
          value={tab}
          onValueChange={(value) => setTab(value as TabId)}
          // Vertical: seven horizontal tabs would wrap in Romanian at the
          // narrowest supported width, and a wrapped tab strip reflows the
          // panel every time the active tab changes row.
          orientation="vertical"
          activationMode="manual"
          className="flex h-full min-h-0 gap-4"
        >
          <TabsList className="flex w-44 shrink-0 flex-col items-stretch gap-0.5 border-r border-b-0 border-[var(--color-border-subtle)] pr-2">
            {tabIds.map((id) => {
              const Icon = tabIcons[id];
              return (
                <TabsTrigger
                  key={id}
                  value={id}
                  className={cn(
                    'h-8 justify-start rounded-[var(--radius-control)] border-b-0 px-2.5',
                    'data-[state=active]:bg-[var(--color-accent-subtle)] data-[state=active]:text-[var(--color-accent-text)]',
                  )}
                >
                  <Icon aria-hidden="true" className="size-4 shrink-0" />
                  <span className="truncate">{labels[id]}</span>
                </TabsTrigger>
              );
            })}
          </TabsList>

          <ScrollArea className="min-h-0 flex-1" ariaLabel={t('settings.title')}>
            <div className="pr-3">
              <TabsContent value="general">
                <GeneralPanel />
              </TabsContent>
              <TabsContent value="appearance">
                <AppearancePanel />
              </TabsContent>
              <TabsContent value="sampling">
                <SamplingPanel />
              </TabsContent>
              <TabsContent value="notifications">
                <NotificationsPanel />
              </TabsContent>
              <TabsContent value="watchdog">
                <WatchdogPanel />
              </TabsContent>
              <TabsContent value="remote">
                <RemoteAccessPanel />
              </TabsContent>
              <TabsContent value="health">
                <HealthPanel />
              </TabsContent>
              <TabsContent value="privacy">
                <PrivacyPanel />
              </TabsContent>
              <TabsContent value="about">
                <AboutPanel version={version} />
              </TabsContent>
            </div>
          </ScrollArea>
        </Tabs>
      </DialogContent>
    </DialogRoot>
  );
}

function requestedTab(): TabId | null {
  const tab = takeSettingsTab();
  return tab !== null && (tabIds as readonly string[]).includes(tab) ? (tab as TabId) : null;
}
