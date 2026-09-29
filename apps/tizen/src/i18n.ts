/**
 * i18next for the TV app. Resources are bundled — a packaged app must not
 * fetch anything to show its own words — and the language is resolved once
 * at start and whenever Settings changes it.
 */

import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';

import en from './locales/en.json';
import ro from './locales/ro.json';

export type LanguageChoice = 'system' | 'en' | 'ro';
export type Language = 'en' | 'ro';

const STORAGE_KEY = 'vitals.language';

export function resolveLanguage(choice: LanguageChoice, systemLanguage: string): Language {
  if (choice !== 'system') return choice;
  return systemLanguage.toLowerCase().startsWith('ro') ? 'ro' : 'en';
}

export function loadLanguageChoice(): LanguageChoice {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved === 'en' || saved === 'ro' ? saved : 'system';
  } catch {
    return 'system';
  }
}

export async function setLanguageChoice(choice: LanguageChoice): Promise<void> {
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // A disabled store only means the choice is forgotten at the next start.
  }
  const language = resolveLanguage(choice, navigator.language);
  document.documentElement.lang = language;
  await i18next.changeLanguage(language);
}

export async function initI18n(): Promise<void> {
  const language = resolveLanguage(loadLanguageChoice(), navigator.language);
  document.documentElement.lang = language;
  await i18next.use(initReactI18next).init({
    resources: { en: { translation: en }, ro: { translation: ro } },
    lng: language,
    fallbackLng: 'en',
    // React escapes on render; escaping here too would double-encode.
    interpolation: { escapeValue: false },
    returnNull: false,
  });
}
