import { createContext, use, useCallback, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';

import { applyTheme } from './apply';
import { defaultTheme, type ThemeSettings } from './types';

interface ThemeContextValue {
  readonly theme: ThemeSettings;
  readonly setTheme: (patch: Partial<ThemeSettings>) => void;
  /** The mode actually in effect, with `system` already resolved. */
  readonly resolvedMode: 'light' | 'dark';
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const DARK_QUERY = '(prefers-color-scheme: dark)';

export function ThemeProvider({
  children,
  initial = defaultTheme,
  onChange,
}: {
  children: ReactNode;
  initial?: ThemeSettings;
  /** Called on every change so the caller can persist. */
  onChange?: (theme: ThemeSettings) => void;
}) {
  const [theme, setThemeState] = useState<ThemeSettings>(initial);
  const [systemPrefersDark, setSystemPrefersDark] = useState(
    () => globalThis.matchMedia?.(DARK_QUERY).matches ?? false,
  );

  // Track the OS preference so `mode: 'system'` follows it live rather than
  // only at startup — users switch themes on a schedule and the app should
  // follow without a restart.
  useEffect(() => {
    const media = globalThis.matchMedia?.(DARK_QUERY);
    if (!media) return;

    const onMediaChange = (event: MediaQueryListEvent) => setSystemPrefersDark(event.matches);
    media.addEventListener('change', onMediaChange);
    return () => media.removeEventListener('change', onMediaChange);
  }, []);

  useEffect(() => {
    applyTheme(theme, document.documentElement, systemPrefersDark);
  }, [theme, systemPrefersDark]);

  const setTheme = useCallback(
    (patch: Partial<ThemeSettings>) => {
      setThemeState((prev) => {
        const next = { ...prev, ...patch };
        onChange?.(next);
        return next;
      });
    },
    [onChange],
  );

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      setTheme,
      resolvedMode: theme.mode === 'system' ? (systemPrefersDark ? 'dark' : 'light') : theme.mode,
    }),
    [theme, setTheme, systemPrefersDark],
  );

  return <ThemeContext value={value}>{children}</ThemeContext>;
}

export function useTheme(): ThemeContextValue {
  const value = use(ThemeContext);
  if (!value) throw new Error('useTheme must be used inside a ThemeProvider');
  return value;
}
