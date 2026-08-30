import i18next, { type i18n as I18nInstance } from 'i18next';
import { initReactI18next } from 'react-i18next';

import en from './locales/en.json';
import ro from './locales/ro.json';

export const locales = ['en', 'ro'] as const;
export type Locale = (typeof locales)[number];

export const defaultLocale: Locale = 'en';

export const i18n: I18nInstance = i18next;

/**
 * Initialises i18next.
 *
 * `escapeValue` is disabled because React already escapes interpolated values;
 * leaving i18next's own escaping on double-encodes anything containing an
 * ampersand — and process command lines are full of them.
 */
export interface InitOptions {
  /**
   * Report missing translation keys.
   *
   * Passed in rather than read from `import.meta.env`, so this package does
   * not depend on a bundler-injected global. That would make it unusable
   * from a plain Node context — the CLI and any future test harness.
   */
  readonly reportMissingKeys?: boolean;
}

export async function initI18n(
  locale: Locale = defaultLocale,
  options: InitOptions = {},
): Promise<I18nInstance> {
  if (i18next.isInitialized) {
    await i18next.changeLanguage(locale);
    return i18next;
  }

  await i18next.use(initReactI18next).init({
    resources: {
      en: { translation: en },
      ro: { translation: ro },
    },
    lng: locale,
    fallbackLng: defaultLocale,
    interpolation: { escapeValue: false },
    // Surface missing keys loudly in development rather than silently
    // rendering the key path, which is easy to miss in a dense UI.
    saveMissing: options.reportMissingKeys ?? false,
    returnNull: false,
  });

  return i18next;
}

/** Whether a string is a locale we ship. */
export function isSupportedLocale(value: string): value is Locale {
  return (locales as readonly string[]).includes(value);
}
