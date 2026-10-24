import type { ReactNode } from 'react';

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ErrorProvider, useError, withErrorHandling } from './ErrorContext';

const wrapper = ({ children }: { children: ReactNode }) => <ErrorProvider>{children}</ErrorProvider>;
const setup = () => renderHook(() => useError(), { wrapper });

beforeEach(() => { vi.spyOn(console, 'error').mockImplementation(() => {}); });
afterEach(() => vi.restoreAllMocks());

describe('ErrorProvider', () => {
  it('requires a provider', () => {
    expect(() => renderHook(() => useError())).toThrow('useError must be used within an ErrorProvider');
  });

  it('shows, dismisses and clears errors', () => {
    const { result } = setup();
    let id = '';
    act(() => { id = result.current.showError('Recipe could not be saved'); });
    act(() => { result.current.showError('Second problem', { severity: 'warning', duration: 0 }); });
    expect(result.current.errors.map((e) => [e.message, e.severity])).toEqual([['Recipe could not be saved', 'error'], ['Second problem', 'warning']]);
    act(() => result.current.dismissError(id));
    expect(result.current.errors.map((e) => e.message)).toEqual(['Second problem']);
    act(() => result.current.clearAll());
    expect(result.current.errors).toEqual([]);
  });

  it.each([
    ['Permission denied: /herbarium\nat open()', 'Permission denied: /herbarium', 'warning', false, 'Learn More'],
    ['File not found: notes.md\ncaused by io', 'File not found: notes.md', 'info', false, undefined],
    ['Network unreachable', 'Network error. Please check your internet connection.', 'info', true, 'Retry'],
    ['Database is corrupted', 'Database error. Please try restarting the application.', 'error', false, undefined],
    ['fatal: index header', 'fatal: index header', 'error', false, undefined],
    ['Request timeout after a while', 'Request timeout after a while', 'warning', true, 'Retry'],
    ['Unsupported image format', 'Unsupported image format', 'warning', false, undefined],
    ['Indexing queue is full', 'Indexing queue is full', 'info', true, 'View Queue'],
    ['Model herb-9b not found', 'Model herb-9b not found', 'info', false, 'Download Model'],
    ['Connection reset by peer', 'Connection reset by peer', 'info', true, undefined],
  ])('turns a backend error %j into a readable toast', (raw, message, severity, recoverable, actionLabel) => {
    const { result } = setup();
    act(() => result.current.handleTauriError(raw));
    expect(result.current.errors[0]).toMatchObject({ message, severity, recoverable });
    expect(result.current.errors[0].action?.label).toBe(actionLabel);
  });

  it('keeps an empty message empty', () => {
    const { result } = setup();
    act(() => result.current.handleTauriError(''));
    expect(result.current.errors[0]).toMatchObject({ message: '', severity: 'info', recoverable: false });
  });

  it('lets the caller override what it would have chosen', () => {
    const { result } = setup();
    const onClick = vi.fn();
    act(() => result.current.handleTauriError('Network unreachable', { severity: 'error', recoverable: false, duration: 100, action: { label: 'Try again', onClick } }));
    expect(result.current.errors[0]).toMatchObject({ severity: 'error', recoverable: false, duration: 100, action: { label: 'Try again' } });
  });

  it('suggested actions open help or navigate', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null);
    const navigate = vi.fn();
    window.addEventListener('navigate', navigate);
    const { result } = setup();
    act(() => {
      result.current.handleTauriError('Permission denied');
      result.current.handleTauriError('Model x not found');
      result.current.handleTauriError('queue is full');
    });
    const [permission, model, queue] = result.current.errors;
    permission.action!.onClick();
    expect(open).toHaveBeenCalledWith(expect.stringContaining('#permission-errors'), '_blank');
    model.action!.onClick();
    queue.action!.onClick();
    expect(navigate.mock.calls.map(([event]) => (event as CustomEvent).detail)).toEqual([{ view: 'settings', section: 'models' }, { view: 'files' }]);
    window.removeEventListener('navigate', navigate);
  });
});

describe('withErrorHandling', () => {
  it('passes results through', async () => {
    const wrapped = withErrorHandling(async (a: number, b: number) => a + b);
    await expect(wrapped(2, 3)).resolves.toBe(5);
  });

  it('hands the failure to the handler and rethrows it as an Error', async () => {
    const handler = vi.fn();
    const wrapped = withErrorHandling(async () => { throw 'oven too hot'; }, handler);
    await expect(wrapped()).rejects.toThrow('oven too hot');
    expect(handler).toHaveBeenCalledWith(expect.objectContaining({ message: 'oven too hot' }));
  });

  it('logs a failure when there is no handler', async () => {
    const failure = new Error('dough collapsed');
    await expect(withErrorHandling(async () => { throw failure; })()).rejects.toBe(failure);
    expect(console.error).toHaveBeenCalled();
  });
});
