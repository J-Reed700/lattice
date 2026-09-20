import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { useApplyTheme, useEffectiveTheme } from '../useApplyTheme';

const settings = vi.hoisted(() => ({ theme: 'system', loaded: true }));
vi.mock('../queries/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: settings.loaded ? { ui: settings } : undefined }),
}));

beforeEach(() => {
  settings.theme = 'system';
  settings.loaded = true;
  localStorage.clear();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
});

it('keeps the cached preference during loading or failure, then trusts the backend', () => {
  localStorage.setItem('lattice-theme', 'dark');
  settings.loaded = false;
  const { result, rerender } = renderHook(() => {
    useApplyTheme();
    return useEffectiveTheme();
  });
  expect(result.current).toBe('dark');
  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  expect(localStorage.getItem('lattice-theme')).toBe('dark');

  settings.loaded = true;
  settings.theme = 'light';
  rerender();
  expect(result.current).toBe('light');
  expect(localStorage.getItem('lattice-theme')).toBe('light');

  settings.theme = 'system';
  rerender();
  expect(localStorage.getItem('lattice-theme')).toBe('system');
});

it('still applies settings when storage is unavailable', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('Unavailable'); });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Unavailable'); });
  settings.loaded = false;
  const { rerender } = renderHook(useApplyTheme);
  expect(document.documentElement).toHaveAttribute('data-theme', 'light');
  settings.loaded = true;
  settings.theme = 'dark';
  rerender();
  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
});

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
