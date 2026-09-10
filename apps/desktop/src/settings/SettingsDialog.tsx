import {
  Bell,
  Info,
  Palette,
  ShieldCheck,
  SlidersHorizontal,
  Timer,
  type LucideIcon,
} from 'lucide-react';
import { useState } from 'react';
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

import { SHELL_NS } from '../shell/strings';
import {
  AboutPanel,
  AppearancePanel,
  GeneralPanel,
  NotificationsPanel,
  PrivacyPanel,
  SamplingPanel,
} from './panels';

const tabIds = ['general', 'appearance', 'sampling', 'notifications', 'privacy', 'about'] as const;

type TabId = (typeof tabIds)[number];

const tabIcons: Readonly<Record<TabId, LucideIcon>> = {
  general: SlidersHorizontal,
  appearance: Palette,
  sampling: Timer,
  notifications: Bell,
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
  const [tab, setTab] = useState<TabId>('general');

  const labels: Readonly<Record<TabId, string>> = {
    general: ts('settings.general.title'),
    appearance: t('settings.appearance.title'),
    sampling: t('settings.sampling.title'),
    notifications: ts('settings.notifications.title'),
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
                    'data-[state=active]:bg-[var(--color-accent-subtle)] data-[state=active]:text-[var(--color-accent)]',
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
