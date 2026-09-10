import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { locales, type Locale } from '@vitals/i18n';
import { Button, Card, Input, SegmentedControl } from '@vitals/ui';

import type { Pairing } from '../lib/pairing';

export interface SettingsScreenProps {
  readonly pairings: readonly Pairing[];
  readonly onRename: (id: string, name: string) => void;
  readonly onRemove: (id: string) => void;
  readonly onLocale: (locale: Locale) => void;
}

/** Paired PCs, language, and how to pair another. */
export function SettingsScreen({ pairings, onRename, onRemove, onLocale }: SettingsScreenProps) {
  const { t, i18n } = useTranslation();
  const current = (locales as readonly string[]).includes(i18n.language)
    ? (i18n.language as Locale)
    : 'en';

  return (
    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 pt-3 pb-4">
      <h1 className="text-lg font-semibold">{t('mobile.settings.title')}</h1>

      <section aria-labelledby="paired-heading" className="space-y-2">
        <h2
          id="paired-heading"
          className="text-2xs font-medium text-[var(--color-fg-muted)] uppercase"
        >
          {t('mobile.settings.paired')}
        </h2>
        {pairings.map((pairing) => (
          <PairingRow key={pairing.id} pairing={pairing} onRename={onRename} onRemove={onRemove} />
        ))}
      </section>

      <section aria-labelledby="language-heading" className="space-y-2">
        <h2
          id="language-heading"
          className="text-2xs font-medium text-[var(--color-fg-muted)] uppercase"
        >
          {t('mobile.settings.language')}
        </h2>
        <SegmentedControl<Locale>
          value={current}
          onValueChange={onLocale}
          ariaLabel={t('mobile.settings.language')}
          options={[
            { value: 'en', label: 'English' },
            { value: 'ro', label: 'Română' },
          ]}
        />
      </section>

      <section aria-labelledby="theme-heading" className="space-y-1">
        <h2
          id="theme-heading"
          className="text-2xs font-medium text-[var(--color-fg-muted)] uppercase"
        >
          {t('mobile.settings.theme')}
        </h2>
        <p className="text-sm text-[var(--color-fg-muted)]">{t('mobile.settings.themeFollows')}</p>
      </section>

      <Card className="p-4">
        <h2 className="text-sm font-semibold">{t('mobile.settings.pairAnother')}</h2>
        <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
          {t('mobile.settings.pairAnotherBody')}
        </p>
      </Card>
    </div>
  );
}

function PairingRow({
  pairing,
  onRename,
  onRemove,
}: {
  readonly pairing: Pairing;
  readonly onRename: (id: string, name: string) => void;
  readonly onRemove: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(pairing.name);
  // Forgetting a PC drops its token; a second tap is required, same as ending
  // a task, because a phone offers no undo.
  const [armed, setArmed] = useState(false);

  return (
    <Card className="p-3">
      {editing ? (
        <form
          className="flex items-end gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            const name = draft.trim();
            if (name !== '') onRename(pairing.id, name);
            setEditing(false);
          }}
        >
          <Input
            className="flex-1"
            label={t('mobile.settings.renameLabel')}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            autoFocus
          />
          <Button type="submit" variant="primary" size="lg">
            {t('mobile.settings.save')}
          </Button>
        </form>
      ) : (
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium">{pairing.name}</div>
            <div className="truncate text-2xs text-[var(--color-fg-muted)]">{pairing.baseUrl}</div>
          </div>
          <Button size="lg" onClick={() => setEditing(true)}>
            {t('mobile.settings.rename')}
          </Button>
          <Button
            size="lg"
            variant={armed ? 'danger' : 'secondary'}
            onClick={() => {
              if (armed) onRemove(pairing.id);
              else setArmed(true);
            }}
            aria-label={
              armed ? t('mobile.settings.removeConfirm', { name: pairing.name }) : undefined
            }
          >
            {t('mobile.settings.remove')}
          </Button>
        </div>
      )}
    </Card>
  );
}
