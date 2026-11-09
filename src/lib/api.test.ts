import { describe, expect, it } from 'vitest';

import { newId, unwrap } from './api';

describe('unwrap', () => {
  it('unwraps a successful CommandResult with { ok: true, data }', () => {
    const data = { id: 'item-1', name: 'Test' };
    const result = unwrap({ ok: true, data });
    expect(result).toBe(data);
  });

  it('throws when CommandResult has { ok: false, error }', () => {
    expect(() => unwrap({ ok: false, error: 'Database connection failed' })).toThrow(
      'Database connection failed',
    );
  });

  it('preserves Error instance when CommandResult error is an Error', () => {
    const error = new Error('Custom error message');
    expect(() => unwrap({ ok: false, error })).toThrow('Custom error message');
  });

  it('unwraps a successful Specta Result with { status: "ok", data }', () => {
    const data = [1, 2, 3];
    const result = unwrap({ status: 'ok', data });
    expect(result).toBe(data);
  });

  it('throws when Specta Result has { status: "error", error }', () => {
    expect(() => unwrap({ status: 'error', error: 'Record not found' })).toThrow(
      'Record not found',
    );
  });

  it('throws readable error when Specta Result error is a structured object with message', () => {
    expect(() =>
      unwrap({ status: 'error', error: { message: 'Validation failed' } }),
    ).toThrow('Validation failed');
  });
});

describe('newId', () => {
  it('returns a valid RFC4122 v4 UUID string', () => {
    const id = newId();
    expect(id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
  });

  it('generates distinct IDs on consecutive calls', () => {
    const id1 = newId();
    const id2 = newId();
    expect(id1).not.toBe(id2);
  });

  it('supports prefix when provided', () => {
    const id = newId('item');
    expect(id).toMatch(
      /^item-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
  });
});
