import { describe, expect, it } from 'vitest';

import { parseApiError } from './errorHandling';

describe('parseApiError', () => {
  it('keeps a backend error as the generated ApiError', () => {
    expect(parseApiError({ code: 'NOT_FOUND', message: 'Space not found: x', details: 'space x' }))
      .toEqual({ code: 'NOT_FOUND', message: 'Space not found: x', details: 'space x' });
  });

  it('reads a serialized backend error and carries structured details as text', () => {
    expect(parseApiError(JSON.stringify({ code: 'DATABASE_ERROR', message: 'Database is locked', details: { operation: 'search' } })))
      .toEqual({ code: 'DATABASE_ERROR', message: 'Database is locked', details: '{"operation":"search"}' });
  });

  it('reports a code the backend does not define as UNKNOWN', () => {
    expect(parseApiError({ code: 'IO', message: 'Disk unavailable' }))
      .toMatchObject({ code: 'UNKNOWN', message: 'Disk unavailable', details: null });
  });

  it('wraps Tauri rejections and renderer exceptions as UNKNOWN', () => {
    expect(parseApiError('plugin search not found')).toEqual({ code: 'UNKNOWN', message: 'plugin search not found' });
    const thrown = parseApiError(new Error('boom'));
    expect(thrown).toMatchObject({ code: 'UNKNOWN', message: 'boom' });
    expect(typeof thrown.details).toBe('string');
  });
});
