/**
 * Internationalisation.
 *
 * English and Romanian at launch; the resource shape is designed so adding a
 * locale is a data change, never a code change.
 *
 * Locale affects more than strings here — number grouping, decimal
 * separators, date order and byte-size formatting all flow from it. Those
 * live in `@vitals/ui`'s formatters, which take the active locale from this
 * module's i18next instance.
 */

export {
  defaultLocale,
  i18n,
  initI18n,
  isSupportedLocale,
  locales,
  type InitOptions,
  type Locale,
} from './config';
