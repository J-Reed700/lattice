/**
 * useApplyTheme Hook
 *
 * Applies theme from settings store to document.
 * Replaces ThemeContext by directly using settingsStore.
 *
 * Usage:
 *   Call once at the app root (App.tsx) to apply theme globally.
 */

import { useEffect } from 'react';

import { useSettingsStore } from '../stores/settingsStore';

export function useApplyTheme() {
  const theme = useSettingsStore((state) => state.settings.display.theme);

  useEffect(() => {
    // Detect system theme preference
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const systemTheme = mediaQuery.matches ? 'dark' : 'light';

    // Calculate effective theme (resolve 'system' to actual theme)
    const effectiveTheme = theme === 'system' ? systemTheme : theme;

    // Apply theme to document
    document.documentElement.setAttribute('data-theme', effectiveTheme);

    // Apply Tailwind dark mode class
    if (effectiveTheme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }

    // Listen for system theme changes (only when theme is 'system')
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
export function useEffectiveTheme(): 'light' | 'dark' {
  const theme = useSettingsStore((state) => state.settings.display.theme);

  if (theme === 'system') {
    const systemTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
    return systemTheme;
  }

  return theme;
}
