import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { isMacOS } from '../../config/shortcuts';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';

describe('Global Shortcuts Integration', () => {
  const modKey = isMacOS() ? { metaKey: true } : { ctrlKey: true };

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('handles multiple shortcuts across different contexts', () => {
    const searchHandler = vi.fn();
    const quickCaptureHandler = vi.fn();
    const rewriteHandler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'search.focus',
          keys: 'Mod+K',
          description: 'Focus search',
          category: 'search',
          handler: searchHandler,
        },
        {
          id: 'quickcapture.open',
          keys: 'Mod+D',
          description: 'Open quick capture',
          category: 'global',
          handler: quickCaptureHandler,
        },
        {
          id: 'queryrewrite.toggle',
          keys: 'Mod+R',
          description: 'Toggle query rewrite',
          category: 'search',
          handler: rewriteHandler,
        },
      ])
    );

    act(() => {
      const event1 = new KeyboardEvent('keydown', {
        key: 'k',
        ...modKey,
        bubbles: true,
      });
      document.body.dispatchEvent(event1);
    });

    expect(searchHandler).toHaveBeenCalledTimes(1);

    act(() => {
      const event2 = new KeyboardEvent('keydown', {
        key: 'd',
        ...modKey,
        bubbles: true,
      });
      document.body.dispatchEvent(event2);
    });

    expect(quickCaptureHandler).toHaveBeenCalledTimes(1);

    act(() => {
      const event3 = new KeyboardEvent('keydown', {
        key: 'r',
        ...modKey,
        bubbles: true,
      });
      document.body.dispatchEvent(event3);
    });

    expect(rewriteHandler).toHaveBeenCalledTimes(1);
  });

  it('handles number key shortcuts for result selection', () => {
    const handlers = {
      1: vi.fn(),
      2: vi.fn(),
      3: vi.fn(),
    };

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'result.select.1',
          keys: '1',
          description: 'Select first result',
          category: 'search',
          handler: handlers['1'],
        },
        {
          id: 'result.select.2',
          keys: '2',
          description: 'Select second result',
          category: 'search',
          handler: handlers['2'],
        },
        {
          id: 'result.select.3',
          keys: '3',
          description: 'Select third result',
          category: 'search',
          handler: handlers['3'],
        },
      ])
    );

    act(() => {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: '1', bubbles: true }));
    });
    expect(handlers['1']).toHaveBeenCalled();

    act(() => {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: '2', bubbles: true }));
    });
    expect(handlers['2']).toHaveBeenCalled();

    act(() => {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }));
    });
    expect(handlers['3']).toHaveBeenCalled();
  });

  it('prevents shortcuts in input fields unless marked as global', () => {
    const normalHandler = vi.fn();
    const globalHandler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'normal.action',
          keys: 'Mod+S',
          description: 'Normal action',
          category: 'editor',
          handler: normalHandler,
          global: false,
        },
        {
          id: 'global.action',
          keys: 'Mod+P',
          description: 'Global action',
          category: 'global',
          handler: globalHandler,
          global: true,
        },
      ])
    );

    const input = document.createElement('input');
    document.body.appendChild(input);

    const event1 = new KeyboardEvent('keydown', {
      key: 's',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      input.dispatchEvent(event1);
    });

    expect(normalHandler).not.toHaveBeenCalled();

    const event2 = new KeyboardEvent('keydown', {
      key: 'p',
      ...modKey,
      bubbles: true,
    });

    act(() => {
      input.dispatchEvent(event2);
    });

    expect(globalHandler).toHaveBeenCalled();

    document.body.removeChild(input);
  });

  it('allows shortcut customization and persistence', () => {
    const handler = vi.fn();

    const { result } = renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'test.customizable',
          keys: 'Mod+T',
          description: 'Customizable action',
          category: 'global',
          handler,
        },
      ])
    );

    act(() => {
      result.current.updateShortcut('test.customizable', 'Mod+Shift+T');
    });

    const customShortcuts = JSON.parse(
      localStorage.getItem('lattice-custom-shortcuts') || '{}'
    );
    expect(customShortcuts['test.customizable']).toBe('Mod+Shift+T');

    act(() => {
      const event = new KeyboardEvent('keydown', {
        key: 't',
        ...modKey,
        shiftKey: true,
        bubbles: true,
      });
      document.body.dispatchEvent(event);
    });

    expect(handler).toHaveBeenCalled();
  });

  it('detects and reports shortcut conflicts', async () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();

    const { result } = renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'action1',
          keys: 'Mod+K',
          description: 'Action 1',
          category: 'global',
          handler: handler1,
        },
        {
          id: 'action2',
          keys: 'Mod+K',
          description: 'Action 2',
          category: 'global',
          handler: handler2,
        },
      ])
    );

    await waitFor(() => {
      expect(result.current.conflicts).toHaveLength(1);
      expect(result.current.conflicts[0]).toEqual({
        shortcut1: 'action1',
        shortcut2: 'action2',
        keys: 'Mod+K',
      });
    });
  });

  it('handles escape key to close panels', () => {
    const closeHandler = vi.fn();

    renderHook(() =>
      useKeyboardShortcuts([
        {
          id: 'panel.close',
          keys: 'Escape',
          description: 'Close panel',
          category: 'global',
          handler: closeHandler,
        },
      ])
    );

    act(() => {
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    });

    expect(closeHandler).toHaveBeenCalled();
  });

  it('supports context-specific shortcuts', () => {
    const editorHandler = vi.fn();
    const searchHandler = vi.fn();

    const { rerender } = renderHook(
      ({ context }: { context: string }) =>
        useKeyboardShortcuts(
          [
            {
              id: 'save',
              keys: 'Mod+S',
              description: 'Save',
              category: context === 'editor' ? 'editor' : 'search',
              handler: context === 'editor' ? editorHandler : searchHandler,
            },
          ],
          { context: context as any }
        ),
      { initialProps: { context: 'editor' } }
    );

    act(() => {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 's', ...modKey, bubbles: true })
      );
    });

    expect(editorHandler).toHaveBeenCalledTimes(1);
    expect(searchHandler).not.toHaveBeenCalled();

    editorHandler.mockClear();
    searchHandler.mockClear();

    rerender({ context: 'search' });

    act(() => {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 's', ...modKey, bubbles: true })
      );
    });

    expect(searchHandler).toHaveBeenCalledTimes(1);
  });

  it('can be enabled/disabled dynamically', () => {
    const handler = vi.fn();

    const { rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) =>
        useKeyboardShortcuts(
          [
            {
              id: 'test.action',
              keys: 'Mod+T',
              description: 'Test action',
              category: 'global',
              handler,
            },
          ],
          { enabled }
        ),
      { initialProps: { enabled: true } }
    );

    act(() => {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 't', ...modKey, bubbles: true })
      );
    });

    expect(handler).toHaveBeenCalledTimes(1);

    handler.mockClear();
    rerender({ enabled: false });

    act(() => {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 't', ...modKey, bubbles: true })
      );
    });

    expect(handler).not.toHaveBeenCalled();
  });
});
