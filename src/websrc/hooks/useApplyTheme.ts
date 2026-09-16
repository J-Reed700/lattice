/**
 * useApplyTheme Hook
 *
 * Applies theme from canonical backend settings (ui.theme) to document.
 *
 * Usage:
 *   Call once at the app root (App.tsx) to apply theme globally.
 */

import { useEffect } from 'react';

import { useSettingsQuery } from './queries/useSettingsQuery';

type ResolvedTheme = 'light' | 'dark';

function resolveSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function normalizeTheme(value: string | undefined): 'light' | 'dark' | 'system' {
  if (value === 'light' || value === 'dark' || value === 'system') return value;
  return 'system';
}

export function useApplyTheme() {
  const { data } = useSettingsQuery();
  const theme = normalizeTheme(data?.ui.theme);

  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const systemTheme = mediaQuery.matches ? 'dark' : 'light';

    const effectiveTheme = theme === 'system' ? systemTheme : theme;

    document.documentElement.setAttribute('data-theme', effectiveTheme);

    if (effectiveTheme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }

    const handleSystemThemeChange = (e: MediaQueryListEvent) => {
      if (theme === 'system') {
        const newSystemTheme = e.matches ? 'dark' : 'light';
        document.documentElement.setAttribute('data-theme', newSystemTheme);

        if (newSystemTheme === 'dark') {
          document.documentElement.classList.add('dark');
        } else {
          document.documentElement.classList.remove('dark');
        }
      }
    };

    mediaQuery.addEventListener('change', handleSystemThemeChange);

    return () => {
      mediaQuery.removeEventListener('change', handleSystemThemeChange);
    };
  }, [theme]);
}

/**
 * Hook to get current effective theme (light or dark)
 * Useful for components that need to know the actual theme being displayed
 */
export function useEffectiveTheme(): ResolvedTheme {
  const { data } = useSettingsQuery();
  const theme = normalizeTheme(data?.ui.theme);

  if (theme === 'system') {
    return resolveSystemTheme();
  }

  return theme;
}
