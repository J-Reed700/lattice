import { describe, expect, it } from 'vitest';

import { isDefined, isError, isNumber, isString, toError } from './typeGuards';

describe('toError', () => {
  it('keeps an Error as it is', () => {
    const error = new Error('spilled');
    expect(toError(error)).toBe(error);
  });

  it('wraps a string', () => {
    expect(toError('flour ran out').message).toBe('flour ran out');
  });

  it('copies the message and stack of an error-like object', () => {
    const converted = toError({ message: 'yeast inactive', stack: 'at proof()' });
    expect(converted.message).toBe('yeast inactive');
    expect(converted.stack).toBe('at proof()');
    expect(toError({ message: 'no stack' }).stack).not.toBe(undefined);
  });

  it('stringifies other objects, or names one that cannot be', () => {
    expect(toError({ code: 7 }).message).toBe('{"code":7}');
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(toError(circular).message).toBe('Unknown error object');
  });

  it('stringifies anything else', () => {
    expect(toError(42).message).toBe('42');
    expect(toError(null).message).toBe('null');
  });
});

describe('type guards', () => {
  it.each([
    [isError, new TypeError('x'), true],
    [isError, { message: 'x' }, false],
    [isString, 'basil', true],
    [isString, 3, false],
    [isNumber, 3, true],
    [isNumber, Number.NaN, false],
    [isNumber, '3', false],
    [isDefined, 0, true],
    [isDefined, null, false],
    [isDefined, undefined, false],
  ] as const)('%o(%o) is %s', (guard, value, expected) => {
    expect((guard as (v: unknown) => boolean)(value)).toBe(expected);
  });
});
