import { describe, expect, it, vi } from 'vitest';

import { handleAsyncEvent, safeAsync, voidAsync } from '../promiseHandlers';

describe('handleAsyncEvent', () => {
  it('contains synchronous failures before a promise is returned', () => {
    const failure = new Error('failed to start');
    const onError = vi.fn();
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const operation = () => { throw failure; };

    expect(() => handleAsyncEvent(operation, onError)()).not.toThrow();
    expect(onError).toHaveBeenCalledWith(failure);
  });

  it('normalizes rejection values and contains a failing error callback', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const callbackFailure = new Error('reporting failed');
    const onError = vi.fn(() => { throw callbackFailure; });

    handleAsyncEvent(async () => { throw { message: 'offline' }; }, onError)();

    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(new Error('offline')));
    expect(consoleError).toHaveBeenCalledWith('Async event error callback failed:', callbackFailure);
  });
  it('passes every event argument to the async callback', async () => {
    const operation = vi.fn(async (_id: string, _force: boolean) => undefined);
    const handler = handleAsyncEvent(operation);

    expect(handler('document-1', true)).toBeUndefined();
    expect(operation).toHaveBeenCalledWith('document-1', true);
  });

  it('logs rejected operations and invokes the optional error callback', async () => {
    const failure = new Error('save failed');
    const operation = vi.fn().mockRejectedValue(failure);
    const onError = vi.fn();
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    handleAsyncEvent(operation, onError)();

    await vi.waitFor(() => {
      expect(onError).toHaveBeenCalledWith(failure);
    });
    expect(consoleError).toHaveBeenCalledWith('Async event handler error:', failure);
  });

  it('still contains a rejection when no custom error callback is supplied', async () => {
    const failure = new Error('network unavailable');
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    handleAsyncEvent(vi.fn().mockRejectedValue(failure))();

    await vi.waitFor(() => {
      expect(consoleError).toHaveBeenCalledWith('Async event handler error:', failure);
    });
  });
});

describe('voidAsync', () => {
  it('contains rejected background operations', async () => {
    const failure = new Error('background task failed');
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    voidAsync(async () => { throw failure; })();
    await vi.waitFor(() => expect(consoleError).toHaveBeenCalledWith('Async event handler error:', failure));
  });
  it('starts the operation with its arguments and returns synchronously', () => {
    const operation = vi.fn(async (_path: string) => 'ignored result');
    const wrapped = voidAsync(operation);

    expect(wrapped('/tmp/note.md')).toBeUndefined();
    expect(operation).toHaveBeenCalledWith('/tmp/note.md');
  });
});

describe('safeAsync', () => {
  it('returns data from a fulfilled operation', async () => {
    await expect(safeAsync(async () => ({ id: 'result-1' }))).resolves.toEqual({
      success: true,
      data: { id: 'result-1' },
    });
  });

  it('preserves Error instances from rejected operations', async () => {
    const failure = new TypeError('invalid response');

    const result = await safeAsync(async () => Promise.reject(failure));

    expect(result).toEqual({ success: false, error: failure });
  });

  it('normalizes non-Error rejection values', async () => {
    const result = await safeAsync(async () => Promise.reject('offline'));

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error).toEqual(new Error('offline'));
    }
  });
});
