/**
 * The `?` sheet: every shortcut the window understands.
 *
 * Rendered from the registry rather than a hand-written list, so a shortcut
 * that exists is documented and a documented one exists. A test asserts every
 * registry entry appears here, which is what stops the two drifting the way
 * they always do when the list is typed out by hand.
 */

import { useTranslation } from 'react-i18next';

import { DialogContent, DialogRoot } from '@vitals/ui';

import { shortcuts } from './shortcuts';
import { SHELL_NS } from './strings';

export function ShortcutsHelp({
  open,
  onOpenChange,
}: {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation(SHELL_NS);

  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="md"
        title={t('shortcuts.title')}
        description={t('shortcuts.subtitle')}
        closeLabel={t('shortcuts.close')}
      >
        <dl className="flex flex-col gap-2">
          {shortcuts.map((shortcut) => (
            <div key={shortcut.id} className="flex items-baseline justify-between gap-4">
              <dt className="min-w-0 text-sm">{t(shortcut.labelKey)}</dt>
              <dd className="flex shrink-0 items-center gap-1">
                {shortcut.keys.map((key) => (
                  <kbd
                    key={key}
                    className="rounded-[var(--radius-control)] border border-[var(--color-border-default)] bg-[var(--color-bg-inset)] px-1.5 py-0.5 text-2xs"
                  >
                    {key}
                  </kbd>
                ))}
              </dd>
            </div>
          ))}
        </dl>
      </DialogContent>
    </DialogRoot>
  );
}
