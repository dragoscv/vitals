import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { loadLanguageChoice, setLanguageChoice, type LanguageChoice } from '../i18n';
import type { Pairing } from '../lib/pairing';
import { useApp, usePairings } from '../ui/app-context';
import { Action, Choice, Hint, InfoRow, Panel, ScreenTitle } from '../ui/components';

const LANGUAGES: readonly LanguageChoice[] = ['system', 'en', 'ro'];
const LANGUAGE_KEYS: Record<LanguageChoice, string> = {
  system: 'settings.languageSystem',
  en: 'settings.languageEn',
  ro: 'settings.languageRo',
};

function PairedRow({ pairing }: { pairing: Pairing }) {
  const { t } = useTranslation();
  const { pairings } = useApp();
  const [armed, setArmed] = useState(false);
  // Removing is two presses, and the first lapses: a stray OK must not
  // forget a PC the user has to walk to in order to pair again.
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 4_000);
    return () => clearTimeout(timer);
  }, [armed]);
  const scope =
    pairing.scope === 'read'
      ? t('settings.readOnly')
      : pairing.scope === 'control'
        ? t('settings.canControl')
        : t('settings.unknownScope');
  return (
    <div className="settings-row">
      <InfoRow
        label={pairing.label}
        value={`${pairing.baseUrl.replace(/^https?:\/\//, '')} · ${scope}`}
      />
      <Action
        label={t(armed ? 'settings.removeConfirm' : 'settings.remove')}
        danger
        onPress={() => (armed ? pairings.remove(pairing.id) : setArmed(true))}
      />
    </div>
  );
}

export function SettingsScreen() {
  const { t } = useTranslation();
  const list = usePairings();
  const [language, setLanguage] = useState<LanguageChoice>(loadLanguageChoice);
  return (
    <div className="screen">
      <ScreenTitle>{t('settings.title')}</ScreenTitle>
      <Panel title={t('settings.pcs')}>
        {list.length === 0 ? (
          <Hint>{t('settings.none')}</Hint>
        ) : (
          list.map((p) => <PairedRow key={p.id} pairing={p} />)
        )}
        <Hint>{t('settings.storage')}</Hint>
      </Panel>
      <Panel title={t('settings.language')}>
        <div className="choice-row">
          {LANGUAGES.map((l, i) => (
            <Choice
              key={l}
              label={t(LANGUAGE_KEYS[l])}
              selected={language === l}
              autoFocus={list.length === 0 && i === 0}
              onPress={() => {
                setLanguage(l);
                void setLanguageChoice(l);
              }}
            />
          ))}
        </div>
      </Panel>
      <Panel title={t('settings.about')}>
        <button type="button" className="panel-stop stack">
          <InfoRow label="Vitals" value={t('settings.version', { version: __APP_VERSION__ })} />
          <Hint>{t('settings.aboutBody')}</Hint>
          <Hint>{t('settings.privacy')}</Hint>
          <Hint>{t('settings.source')}</Hint>
        </button>
      </Panel>
    </div>
  );
}
