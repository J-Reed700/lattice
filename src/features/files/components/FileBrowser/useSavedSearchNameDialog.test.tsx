import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useSavedSearchNameDialog } from './useSavedSearchNameDialog';

const h = vi.hoisted(() => ({
  createSavedSearch: vi.fn(),
  updateSavedSearch: vi.fn(),
  toast: { warning: vi.fn() },
}));
vi.mock('@/features/files/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (selector: (state: unknown) => unknown) =>
    selector({ createSavedSearch: h.createSavedSearch, updateSavedSearch: h.updateSavedSearch }),
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

describe('useSavedSearchNameDialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('opens with a mode, value and target, and closes clean', () => {
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('rename', 'Roman roads', 's1'));
    expect(result.current.savedSearchNameDialogMode).toBe('rename');
    expect(result.current.savedSearchNameDialogValue).toBe('Roman roads');
    act(() => result.current.closeSavedSearchNameDialog());
    expect(result.current.savedSearchNameDialogMode).toBeNull();
    expect(result.current.savedSearchNameDialogValue).toBe('');
  });

  it('refuses an empty name', () => {
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('create', '   '));
    act(() => result.current.submitSavedSearchNameDialog());
    expect(h.toast.warning).toHaveBeenCalledWith('Search name cannot be empty');
    expect(h.createSavedSearch).not.toHaveBeenCalled();
  });

  it('creates a saved search with the trimmed name and closes', () => {
    h.createSavedSearch.mockReturnValueOnce('new-id');
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('create', ''));
    act(() => result.current.setSavedSearchNameDialogValue('  Sourdough  '));
    act(() => result.current.submitSavedSearchNameDialog());
    expect(h.createSavedSearch).toHaveBeenCalledWith('Sourdough');
    expect(result.current.savedSearchNameDialogMode).toBeNull();
  });

  it('keeps the dialog open when the store refuses the name', () => {
    h.createSavedSearch.mockReturnValueOnce(null);
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('create', 'Duplicate'));
    act(() => result.current.submitSavedSearchNameDialog());
    expect(h.toast.warning).toHaveBeenCalledWith('Saved search not created', {
      message: 'That name is already in use, or nothing is being searched.',
    });
    expect(result.current.savedSearchNameDialogMode).toBe('create');
  });

  it('renames the target search', () => {
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('rename', 'Glaciers', 's9'));
    act(() => result.current.submitSavedSearchNameDialog());
    expect(h.updateSavedSearch).toHaveBeenCalledWith('s9', { name: 'Glaciers' });
    expect(result.current.savedSearchNameDialogMode).toBeNull();
  });

  it('does nothing for a rename with no target', () => {
    const { result } = renderHook(() => useSavedSearchNameDialog());
    act(() => result.current.openSavedSearchNameDialog('rename', 'Glaciers'));
    act(() => result.current.submitSavedSearchNameDialog());
    expect(h.updateSavedSearch).not.toHaveBeenCalled();
    expect(result.current.savedSearchNameDialogMode).toBe('rename');
  });
});
