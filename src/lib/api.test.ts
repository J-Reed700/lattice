import { describe, expect, it } from 'vitest';

import { isConflict, newId, unwrap } from './api';

describe('unwrap', () => {
  it('returns the data of a successful result', () => {
    const data = { id: 'item-1' };
    expect(unwrap({ ok: true, data })).toBe(data);
  });

  it('throws the message of a failed result and keeps its details', () => {
    const details = { code: 'CONFLICT' } as never;
    let thrown: unknown;
    try {
      unwrap({ ok: false, error: 'Changed elsewhere.', details });
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(Error);
    expect((thrown as Error).message).toBe('Changed elsewhere.');
    expect(isConflict(thrown)).toBe(true);
  });

  it('reads the Specta result shape too', () => {
    expect(unwrap({ status: 'ok', data: [1, 2] })).toEqual([1, 2]);
    expect(() => unwrap({ status: 'error', error: { message: 'Not found' } })).toThrow('Not found');
  });
});

describe('newId', () => {
  it('returns a UUID, different each time', () => {
    const first = newId();
    expect(first).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
    expect(newId()).not.toBe(first);
  });
});

describe('isConflict', () => {
  it('is true only for the CONFLICT code in the error details', () => {
    expect(isConflict({ details: { code: 'CONFLICT' } })).toBe(true);
    expect(isConflict({ details: { code: 'NOT_FOUND' } })).toBe(false);
  });

  it('does not read the message', () => {
    expect(isConflict(new Error('conflict: revision changed'))).toBe(false);
    expect(isConflict(null)).toBe(false);
    expect(isConflict('CONFLICT')).toBe(false);
  });
});
