import { describe, expect, it } from 'vitest';

import { describeFailure, parseApiError } from './errorHandling';

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

describe('describeFailure', () => {
  it('follows a generic message with the backend detail', () => {
    expect(describeFailure({
      ok: false,
      error: 'Network error',
      details: { code: 'NETWORK_ERROR', message: 'Network error', details: 'connection reset' },
    })).toBe('Network error: connection reset');
  });

  it('keeps a message that already carries its detail, or has none', () => {
    expect(describeFailure({
      ok: false,
      error: 'Not found: Download session not found: d1',
      details: { code: 'NOT_FOUND', message: 'Not found: Download session not found: d1', details: null },
    })).toBe('Not found: Download session not found: d1');
    expect(describeFailure({
      ok: false,
      error: 'Content extraction failed: a.pdf (bad xref)',
      details: { code: 'EXTRACTION_ERROR', message: 'Content extraction failed: a.pdf (bad xref)', details: 'bad xref' },
    })).toBe('Content extraction failed: a.pdf (bad xref)');
    expect(describeFailure({ ok: false, error: 'database is locked' })).toBe('database is locked');
  });
});
