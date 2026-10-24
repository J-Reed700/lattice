import { act, fireEvent, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { logger } from '../../utils/logger';
import { useIsShortcutAvailable, useKeyboardShortcuts, useRegisteredShortcuts } from '../useKeyboardShortcuts';

import type { KeyboardShortcutRegistration } from '../../types/shortcuts';

const shortcut = (id: string, keys: string, extra: Partial<KeyboardShortcutRegistration> = {}): KeyboardShortcutRegistration => ({
  id, keys, description: id, handler: vi.fn(), ...extra,
} as KeyboardShortcutRegistration);

const press = (key: string, target: Element = document.body) => fireEvent.keyDown(target, { key, ctrlKey: true });

beforeEach(() => {
  localStorage.removeItem('lattice-custom-shortcuts');
  vi.spyOn(logger, 'error').mockImplementation(() => {});
  vi.spyOn(logger, 'warn').mockImplementation(() => {});
  vi.spyOn(logger, 'info').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('useKeyboardShortcuts registry', () => {
  it('lists registered shortcuts and says which keys are free', () => {
    const save = shortcut('recipe.save', 'Mod+S');
    const { unmount } = renderHook(() => useKeyboardShortcuts([save]));
    expect(renderHook(() => useRegisteredShortcuts()).result.current.map((s) => s.id)).toContain('recipe.save');
    expect(renderHook(() => useIsShortcutAvailable('Mod+S')).result.current).toBe(false);
    expect(renderHook(() => useIsShortcutAvailable('Mod+S', 'recipe.save')).result.current).toBe(true);
    expect(renderHook(() => useIsShortcutAvailable('Mod+J')).result.current).toBe(true);
    unmount();
    expect(renderHook(() => useIsShortcutAvailable('Mod+S')).result.current).toBe(true);
  });

  it('reports shortcuts that clash in the same context, but not across contexts', () => {
    const first = renderHook(() => useKeyboardShortcuts([shortcut('a.save', 'Mod+S')], { context: 'editor' }));
    const other = renderHook(() => useKeyboardShortcuts([shortcut('b.save', 'Mod+S')], { context: 'search' }));
    expect(other.result.current.conflicts).toEqual([]);
    const clash = renderHook(() => useKeyboardShortcuts([shortcut('c.save', 'Mod+S')], { context: 'editor' }));
    expect(clash.result.current.conflicts).toEqual([{ shortcut1: 'a.save', shortcut2: 'c.save', keys: 'Mod+S' }]);
    expect(logger.warn).toHaveBeenCalledWith('Shortcut conflict detected for "c.save":', expect.anything());
    first.unmount();
    other.unmount();
    clash.unmount();
  });

  it('only runs shortcuts for its own context and ignores events not from page elements', () => {
    const editorOnly = shortcut('editor.bold', 'Mod+B', { options: { context: 'editor' } } as Partial<KeyboardShortcutRegistration>);
    const { unmount } = renderHook(() => useKeyboardShortcuts([editorOnly], { context: 'search' }));
    press('b');
    expect(editorOnly.handler).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: 'b', ctrlKey: true });
    expect(editorOnly.handler).not.toHaveBeenCalled();
    unmount();
  });

  it('logs a shortcut whose handler throws', () => {
    const broken = shortcut('broken', 'Mod+K', { handler: vi.fn(() => { throw new Error('boom'); }) });
    const { unmount } = renderHook(() => useKeyboardShortcuts([broken]));
    press('k');
    expect(logger.error).toHaveBeenCalledWith('Error executing shortcut "broken":', expect.objectContaining({ error: expect.any(Error) }));
    unmount();
  });

  it('remaps, resets and resets every shortcut', () => {
    const save = shortcut('recipe.save', 'Mod+S');
    const print = shortcut('recipe.print', 'Mod+P');
    const { result, unmount } = renderHook(() => useKeyboardShortcuts([save, print]));
    act(() => result.current.updateShortcut('recipe.save', 'Mod+E'));
    act(() => result.current.updateShortcut('recipe.print', 'Mod+R'));
    expect(JSON.parse(localStorage.getItem('lattice-custom-shortcuts')!)).toEqual({ 'recipe.save': 'Mod+E', 'recipe.print': 'Mod+R' });
    press('e');
    expect(save.handler).toHaveBeenCalledTimes(1);
    act(() => result.current.resetShortcut('recipe.save'));
    press('s');
    expect(save.handler).toHaveBeenCalledTimes(2);
    expect(renderHook(() => useIsShortcutAvailable('Mod+S')).result.current).toBe(false);
    act(() => result.current.resetAllShortcuts());
    expect(localStorage.getItem('lattice-custom-shortcuts')).toBeNull();
    expect(renderHook(() => useIsShortcutAvailable('Mod+P')).result.current).toBe(false);
    act(() => result.current.updateShortcut('not.registered', 'Mod+Q'));
    act(() => result.current.resetShortcut('not.registered'));
    unmount();
  });

  it('falls back to the defaults when saved shortcuts cannot be read or written', () => {
    localStorage.setItem('lattice-custom-shortcuts', '{broken');
    const save = shortcut('recipe.save', 'Mod+S');
    const { result, unmount } = renderHook(() => useKeyboardShortcuts([save]));
    press('s');
    expect(save.handler).toHaveBeenCalled();
    expect(logger.error).toHaveBeenCalledWith('Failed to load custom shortcuts:', expect.anything());
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
    act(() => result.current.updateShortcut('recipe.save', 'Mod+E'));
    expect(logger.error).toHaveBeenCalledWith('Failed to save custom shortcuts:', expect.anything());
    unmount();
  });
});
