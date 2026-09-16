/**
 * useApplyTheme Hook
 *
 * Applies theme from canonical backend settings (ui.theme) to document.
 *
 * Usage:
 *   Call once at the app root (App.tsx) to apply theme globally.
 */

import { useEffect, useSyncExternalStore } from 'react';

import { useSettingsQuery } from './queries/useSettingsQuery';

type ResolvedTheme = 'light' | 'dark';

function resolveSystemTheme(): ResolvedTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function normalizeTheme(value: string | undefined): 'light' | 'dark' | 'system' {
  if (value === 'light' || value === 'dark' || value === 'system') return value;
  return 'system';
}

function subscribeSystemTheme(onChange: () => void): () => void {
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
}

export function useApplyTheme() {
  const theme = useEffectiveTheme();
  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute('data-theme', theme);
    root.classList.toggle('dark', theme === 'dark');
    root.style.colorScheme = theme;
  }, [theme]);
}

/** Keep JS-rendered previews in sync with the same theme as the app shell. */
export function useEffectiveTheme(): ResolvedTheme {
  const { data } = useSettingsQuery();
  const theme = normalizeTheme(data?.ui.theme);
  const systemTheme = useSyncExternalStore<ResolvedTheme>(subscribeSystemTheme, resolveSystemTheme, () => 'light');
  return theme === 'system' ? systemTheme : theme;
}
