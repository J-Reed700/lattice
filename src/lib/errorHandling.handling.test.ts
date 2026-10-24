import { afterEach, describe, expect, it, vi } from 'vitest';

import { ErrorCode } from '@/types/api/errorCodes';

import { apiErrorToException, handleApiError, parseApiError } from './errorHandling';

afterEach(() => vi.restoreAllMocks());

describe('parseApiError fallbacks', () => {
  it('reads plain text as an unknown error', () => {
    expect(parseApiError('Oven offline')).toEqual({ code: ErrorCode.UNKNOWN, message: 'Oven offline' });
  });

  it('gives anything else a generic message with its text as detail', () => {
    expect(parseApiError(42)).toEqual({ code: ErrorCode.UNKNOWN, message: 'An unexpected error occurred', details: '42' });
  });

  it('writes details that JSON cannot as text', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(parseApiError({ code: 'NOT_FOUND', message: '', details: circular })).toEqual({
      code: ErrorCode.NOT_FOUND, message: 'Unknown error', details: '[object Object]',
    });
  });
});

describe('handleApiError', () => {
  it('logs the error and the message the user would see', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    handleApiError({ code: ErrorCode.NOT_FOUND, message: 'gone', details: 'recipe 7' }, 'loading recipe');
    expect(error).toHaveBeenCalledWith('[API Error]', { code: ErrorCode.NOT_FOUND, message: 'gone', details: 'recipe 7', context: 'loading recipe' });
    expect(error).toHaveBeenCalledWith('[User Message]', expect.any(String));
  });

  it('falls back to the unknown message for a code it does not know', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    handleApiError({ code: 'MADE_UP' as ErrorCode, message: 'x' });
    const unknown = vi.mocked(console.error).mock.calls.find(([label]) => label === '[User Message]')?.[1];
    error.mockClear();
    handleApiError({ code: ErrorCode.UNKNOWN, message: 'y' });
    expect(vi.mocked(console.error).mock.calls.find(([label]) => label === '[User Message]')?.[1]).toBe(unknown);
  });
});

describe('apiErrorToException', () => {
  it('keeps the code and details on the thrown error', () => {
    const exception = apiErrorToException({ code: ErrorCode.NOT_FOUND, message: 'gone', details: 'recipe 7' });
    expect(exception).toBeInstanceOf(Error);
    expect(exception).toMatchObject({ name: ErrorCode.NOT_FOUND, message: 'gone', code: ErrorCode.NOT_FOUND, details: 'recipe 7' });
    expect(apiErrorToException({ code: ErrorCode.UNKNOWN, message: 'plain' })).not.toHaveProperty('details');
  });
});
