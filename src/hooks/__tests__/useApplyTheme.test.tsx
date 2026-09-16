import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import { useApplyTheme, useEffectiveTheme } from '../useApplyTheme';

const settings = vi.hoisted(() => ({ theme: 'system' }));
vi.mock('../queries/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: { ui: settings } }),
}));

afterEach(() => { vi.unstubAllGlobals(); });

it('updates both document chrome and JS consumers when the OS theme changes', () => {
  let dark = false;
  const listeners = new Set<() => void>();
  vi.stubGlobal('matchMedia', () => ({
    get matches() { return dark; },
    addEventListener: (_: string, callback: () => void) => listeners.add(callback),
    removeEventListener: (_: string, callback: () => void) => listeners.delete(callback),
  }));
  const { result, rerender, unmount } = renderHook(() => {
    useApplyTheme();
    return useEffectiveTheme();
  });
  expect(result.current).toBe('light');
  act(() => { dark = true; listeners.forEach((callback) => callback()); });
  expect(result.current).toBe('dark');
  expect(document.documentElement).toHaveClass('dark');
  expect(document.documentElement.style.colorScheme).toBe('dark');
  settings.theme = 'light';
  rerender();
  expect(result.current).toBe('light');
  expect(document.documentElement).not.toHaveClass('dark');
  act(() => { dark = false; listeners.forEach((callback) => callback()); });
  act(() => { dark = true; listeners.forEach((callback) => callback()); });
  expect(result.current).toBe('light');
  unmount();
  expect(listeners.size).toBe(0);
});
