/**
 * Tests for useKeyboardShortcuts Hook
 *
 * Purpose: Demonstrate shortcut registration and execution
 */

import { renderHook, act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { isMacOS, matchesShortcut } from '../../config/shortcuts';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

describe('useKeyboardShortcuts', () => {
  const modKey = isMacOS() ? { metaKey: true } : { ctrlKey: true };

  beforeEach(() => {
    // Clear localStorage before each test
    localStorage.clear();
  });

  it('should register shortcuts', () => {
    const handler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.action',
          keys: 'Mod+K',
          description: 'Test action',
          category: 'global',
          handler,
        },
      ])
    );

    // Simulate Ctrl+K (or Cmd+K on Mac)
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(handler).toHaveBeenCalled();
  });

  it('should detect conflicts', () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();

    const { result } = renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.action1',
          keys: 'Mod+K',
          description: 'Test action 1',
          category: 'global',
          handler: handler1,
        },
        {
          id: 'test.action2',
          keys: 'Mod+K',
          description: 'Test action 2',
          category: 'global',
          handler: handler2,
        },
      ])
    );

    return waitFor(() => {
      expect(result.current.conflicts).toHaveLength(1);
      expect(result.current.conflicts[0]).toEqual({
        shortcut1: 'test.action1',
        shortcut2: 'test.action2',
        keys: 'Mod+K',
      });
    });
  });

  it('should allow custom shortcuts', () => {
    const handler = vi.fn();

    const { result } = renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.customizable',
          keys: 'Mod+J',
          description: 'Customizable action',
          category: 'global',
          handler,
        },
      ])
    );

    // Update shortcut
    act(() => {
      result.current.updateShortcut('test.customizable', 'Mod+Shift+J');
    });

    // Simulate new shortcut
    const event = new KeyboardEvent('keydown', {
      key: 'j',
      ...modKey,
      shiftKey: true,
      bubbles: true,
    });

    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(handler).toHaveBeenCalled();
  });

  it('should reset shortcuts to default', () => {
    const handler = vi.fn();

    const { result } = renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.resettable',
          keys: 'Mod+R',
          description: 'Resettable action',
          category: 'global',
          handler,
        },
      ])
    );

    // Customize shortcut
    act(() => {
      result.current.updateShortcut('test.resettable', 'Mod+Shift+R');
    });

    // Reset shortcut
    act(() => {
      result.current.resetShortcut('test.resettable');
    });

    // Original shortcut should work
    const event = new KeyboardEvent('keydown', {
      key: 'r',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(handler).toHaveBeenCalled();
  });

  it('should respect context', () => {
    const editorHandler = vi.fn();
    const searchHandler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts(
        [
          {
            id: 'editor.save',
            keys: 'Mod+S',
            description: 'Save in editor',
            category: 'editor',
            handler: editorHandler,
          },
        ],
        { context: 'editor' }
      )
    );

    renderHook(() =>
      useKeyboardShortcuts(
        [
          {
            id: 'search.execute',
            keys: 'Mod+S',
            description: 'Execute search',
            category: 'search',
            handler: searchHandler,
          },
        ],
        { context: 'search' }
      )
    );

    // Trigger shortcut
    const event = new KeyboardEvent('keydown', {
      key: 's',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      document.body.dispatchEvent(event);
    });

    // Only the first registered handler in the current context should be called
    // (In real app, only one context would be active at a time)
    expect(editorHandler).toHaveBeenCalled();
  });

  it('should prevent default when requested', () => {
    const handler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts(
        [
          {
            id: 'test.preventDefault',
            keys: 'Mod+S',
            description: 'Prevent default',
            category: 'global',
            handler,
          },
        ],
        { preventDefault: true }
      )
    );

    const event = new KeyboardEvent('keydown', {
      key: 's',
      ...modKey,
      bubbles: true,
      cancelable: true,
    });

    const preventDefaultSpy = vi.spyOn(event, 'preventDefault');

    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(preventDefaultSpy).toHaveBeenCalled();
    expect(handler).toHaveBeenCalled();
  });

  it('should not trigger when disabled', () => {
    const handler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts(
        [
          {
            id: 'test.disabled',
            keys: 'Mod+D',
            description: 'Disabled shortcut',
            category: 'global',
            handler,
          },
        ],
        { enabled: false }
      )
    );

    const event = new KeyboardEvent('keydown', {
      key: 'd',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      document.body.dispatchEvent(event);
    });

    expect(handler).not.toHaveBeenCalled();
  });

  it('should not trigger in input fields (unless global)', () => {
    const handler = vi.fn();
    const globalHandler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.regular',
          keys: 'Mod+K',
          description: 'Regular shortcut',
          category: 'global',
          handler,
          global: false,
        },
        {
          id: 'test.global',
          keys: 'Mod+P',
          description: 'Global shortcut',
          category: 'global',
          handler: globalHandler,
          global: true,
        },
      ])
    );

    // Create input element
    const input = document.createElement('input');
    document.body.appendChild(input);

    // Simulate typing in input
    const event1 = new KeyboardEvent('keydown', {
      key: 'k',
      ...modKey,
      bubbles: true,
    });
    Object.defineProperty(event1, 'target', { value: input });

    act(() => {
      input.dispatchEvent(event1);
    });

    expect(handler).not.toHaveBeenCalled(); // Should not trigger in input

    // Global shortcut should still work
    const event2 = new KeyboardEvent('keydown', {
      key: 'p',
      ...modKey,
      bubbles: true,
    });
    Object.defineProperty(event2, 'target', { value: input });

    act(() => {
      input.dispatchEvent(event2);
    });

    expect(globalHandler).toHaveBeenCalled(); // Should trigger even in input

    document.body.removeChild(input);
  });
});

describe('matchesShortcut', () => {
  const modKey = isMacOS() ? { metaKey: true } : { ctrlKey: true };

  it('should match simple key', () => {
    const event = new KeyboardEvent('keydown', { key: 'Enter' });
    expect(matchesShortcut(event, 'Enter')).toBe(true);
  });

  it('should match modifier + key', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      ...modKey,
    });
    expect(matchesShortcut(event, 'Mod+K')).toBe(true);
  });

  it('should match multiple modifiers', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'c',
      ...modKey,
      shiftKey: true,
    });
    expect(matchesShortcut(event, 'Mod+Shift+C')).toBe(true);
  });

  it('should not match different key', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      ...modKey,
    });
    expect(matchesShortcut(event, 'Mod+J')).toBe(false);
  });

  it('should not match missing modifier', () => {
    const event = new KeyboardEvent('keydown', { key: 'k' });
    expect(matchesShortcut(event, 'Mod+K')).toBe(false);
  });

  it('should not match extra modifier', () => {
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      ...modKey,
      shiftKey: true,
    });
    expect(matchesShortcut(event, 'Mod+K')).toBe(false);
  });
});
