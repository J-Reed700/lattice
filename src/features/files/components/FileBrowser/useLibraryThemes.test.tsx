import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata, LibraryScope } from '@/types/fileBrowser';

import { useLibraryThemes } from './useLibraryThemes';

const h = vi.hoisted(() => ({
  themes: undefined as unknown[] | undefined,
  enabledArg: null as null | boolean,
  rebuild: { mutate: vi.fn(), isPending: false },
  progressArg: null as null | boolean,
}));

vi.mock('@/features/files/hooks/useClustersQuery', () => ({
  THEMES_MIN_DOCUMENTS: 3,
  useClustersQuery: (enabled: boolean) => { h.enabledArg = enabled; return { data: h.themes }; },
  useRebuildThemes: () => h.rebuild,
  useRebuildProgress: (running: boolean) => { h.progressArg = running; return running ? { phase: 'clustering' } : null; },
}));

const docs = (count: number) => Array.from({ length: count }, (_v, i) => ({ id: String(i) }) as DocumentMetadata);

describe('useLibraryThemes', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.themes = undefined;
    h.rebuild.isPending = false;
  });

  it('stays off below the document threshold', () => {
    const { result } = renderHook(() => useLibraryThemes({ documents: docs(2), scope: { kind: 'all' } as LibraryScope, setScope: vi.fn() }));
    expect(h.enabledArg).toBe(false);
    expect(result.current).toMatchObject({ themesEnabled: false, themes: [], hasRunThemes: false, isFindingThemes: false, themeProgress: null });
  });

  it('lists themes and finds them on request, with progress', () => {
    h.themes = [{ id: 't1', label: 'Grains' }];
    h.rebuild.isPending = true;
    const { result } = renderHook(() => useLibraryThemes({ documents: docs(3), scope: { kind: 'all' } as LibraryScope, setScope: vi.fn() }));
    expect(result.current).toMatchObject({ themesEnabled: true, hasRunThemes: true, isFindingThemes: true, themeProgress: { phase: 'clustering' } });
    act(() => result.current.handleFindThemes());
    expect(h.rebuild.mutate).toHaveBeenCalled();
  });

  it('leaves a theme that no longer exists', () => {
    h.themes = [{ id: 't1', label: 'Grains' }];
    const setScope = vi.fn();
    renderHook(() => useLibraryThemes({ documents: docs(3), scope: { kind: 'theme', id: 'gone' } as LibraryScope, setScope }));
    expect(setScope).toHaveBeenCalledWith({ kind: 'all' });
  });

  it('keeps a theme scope while themes load or when it still exists', () => {
    const setScope = vi.fn();
    const { rerender } = renderHook(({ scope }) => useLibraryThemes({ documents: docs(3), scope, setScope }), {
      initialProps: { scope: { kind: 'theme', id: 't1' } as LibraryScope },
    });
    h.themes = [{ id: 't1', label: 'Grains' }];
    rerender({ scope: { kind: 'theme', id: 't1' } as LibraryScope });
    expect(setScope).not.toHaveBeenCalled();
  });
});
