import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  dismissAllToasts,
  dismissToast,
  showErrorToast,
  showInfoToast,
  showPromiseToast,
  showSuccessToast,
  showWarningToast,
} from '../toast';

const toast = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
  dismiss: vi.fn(),
  dismissAll: vi.fn(),
}));

vi.mock('../../stores/toastStore', () => ({ toast }));

describe('toast helpers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    toast.success.mockReturnValue('success-id');
    toast.error.mockReturnValue('error-id');
    toast.warning.mockReturnValue('warning-id');
    toast.info.mockReturnValue('loading-id');
  });

  it('delegates success, warning, and info messages with their options', () => {
    const action = { label: 'Open', onClick: vi.fn() };

    expect(showSuccessToast('Saved', { duration: 1000, action })).toBe('success-id');
    expect(showWarningToast('Almost full', { message: '90%', duration: 2000, action }))
      .toBe('warning-id');
    expect(showInfoToast('Indexing', { message: '3 files', duration: 0, action }))
      .toBe('loading-id');

    expect(toast.success).toHaveBeenCalledWith('Saved', { duration: 1000, action });
    expect(toast.warning).toHaveBeenCalledWith('Almost full', {
      message: '90%', duration: 2000, action,
    });
    expect(toast.info).toHaveBeenCalledWith('Indexing', {
      message: '3 files', duration: 0, action,
    });
  });

  it.each([
    [new Error('disk full'), 'disk full'],
    ['offline', 'offline'],
    [{ message: 503 }, '503'],
    [{ code: 'UNKNOWN' }, undefined],
    [null, undefined],
  ])('extracts error details from %j', (error, expectedMessage) => {
    expect(showErrorToast('Could not save', error, { duration: 5000 })).toBe('error-id');
    expect(toast.error).toHaveBeenLastCalledWith('Could not save', {
      duration: 5000,
      message: expectedMessage,
    });
  });

  it('replaces a loading toast with a static success message', async () => {
    await expect(showPromiseToast(Promise.resolve(42), {
      loading: 'Loading',
      success: 'Loaded',
      error: 'Failed',
    })).resolves.toBe(42);

    expect(toast.info).toHaveBeenCalledWith('Loading', { duration: 0 });
    expect(toast.dismiss).toHaveBeenCalledWith('loading-id');
    expect(toast.success).toHaveBeenCalledWith('Loaded');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('builds a success message from the fulfilled value', async () => {
    await showPromiseToast(Promise.resolve({ count: 3 }), {
      loading: 'Importing',
      success: ({ count }) => `Imported ${count}`,
      error: 'Failed',
    });

    expect(toast.success).toHaveBeenCalledWith('Imported 3');
  });

  it('replaces loading with a computed error and rethrows the same Error', async () => {
    const failure = new Error('timeout');
    const operation = showPromiseToast(Promise.reject(failure), {
      loading: 'Importing',
      success: 'Imported',
      error: (error) => `Failed: ${error.message}`,
    });

    await expect(operation).rejects.toBe(failure);
    expect(toast.dismiss).toHaveBeenCalledWith('loading-id');
    expect(toast.error).toHaveBeenCalledWith('Failed: timeout', { message: 'timeout' });
  });

  it('uses a static failure message and omits details for non-Error rejections', async () => {
    const operation = showPromiseToast(Promise.reject('offline'), {
      loading: 'Loading',
      success: 'Loaded',
      error: 'Unavailable',
    });

    await expect(operation).rejects.toBe('offline');
    expect(toast.error).toHaveBeenCalledWith('Unavailable', { message: undefined });
  });

  it('delegates individual and bulk dismissal', () => {
    dismissToast('toast-7');
    dismissAllToasts();

    expect(toast.dismiss).toHaveBeenCalledWith('toast-7');
    expect(toast.dismissAll).toHaveBeenCalledOnce();
  });
});
