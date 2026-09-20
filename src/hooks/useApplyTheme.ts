/**
 * useApplyTheme Hook
 *
 * Applies theme from canonical backend settings (ui.theme) to document.
 *
 * Usage:
 *   Call once at the app root (App.tsx) to apply theme globally.
 */

import { useLayoutEffect, useSyncExternalStore } from 'react';

import { useSettingsQuery } from './queries/useSettingsQuery';

type ResolvedTheme = 'light' | 'dark';
// Also read by public/theme-bootstrap.js before the application bundle loads.
const THEME_CACHE_KEY = 'lattice-theme';

function resolveSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function normalizeTheme(value: string | undefined): 'light' | 'dark' | 'system' {
  if (value === 'light' || value === 'dark' || value === 'system') return value;
  return 'system';
}

function cachedTheme() {
  try {
    return normalizeTheme(localStorage.getItem(THEME_CACHE_KEY) ?? undefined);
  } catch {
    return 'system';
  }
}

function subscribeSystemTheme(onChange: () => void): () => void {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
}

export function useApplyTheme() {
  const { data } = useSettingsQuery();
  const theme = useEffectiveTheme();
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.setAttribute('data-theme', theme);
    root.classList.toggle('dark', theme === 'dark');
    root.style.colorScheme = theme;
    // Only cache settings confirmed by the backend, never a pending selection
    // or the temporary fallback while settings are still loading.
    if (data) {
      try {
        localStorage.setItem(THEME_CACHE_KEY, normalizeTheme(data.ui.theme));
      } catch {
        // A storage failure must not prevent applying the saved preference.
      }
    }
  }, [data, theme]);
}

/** Keep JS-rendered previews in sync with the same theme as the app shell. */
export function useEffectiveTheme(): ResolvedTheme {
  const { data } = useSettingsQuery();
  const theme = data ? normalizeTheme(data.ui.theme) : cachedTheme();
  const systemTheme = useSyncExternalStore<ResolvedTheme>(subscribeSystemTheme, resolveSystemTheme, () => 'light');
  return theme === 'system' ? systemTheme : theme;
}
