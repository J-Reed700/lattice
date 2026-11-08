import { describe, expect, it } from 'vitest';

import { ErrorCode } from '@/types/api/errorCodes';

import { apiErrorToException, refusalSentence } from './errorHandling';

describe('refusalSentence', () => {
  it('drops the category the backend puts before an invalid input', () => {
    expect(refusalSentence({ code: ErrorCode.INVALID_INPUT, message: "Invalid input: The question's stem gives away its answer." }))
      .toBe("The question's stem gives away its answer.");
  });

  it('drops the category the backend puts before an invalid state', () => {
    expect(refusalSentence({ code: ErrorCode.INVALID_STATE, message: 'Invalid state: This section changed since you opened it.' }))
      .toBe('This section changed since you opened it.');
  });

  it('reads the code a failed call carries on its error', () => {
    const error = apiErrorToException({ code: ErrorCode.INVALID_INPUT, message: 'Invalid input: A question needs a prompt.', details: null });
    expect(refusalSentence(error)).toBe('A question needs a prompt.');
  });

  it('leaves every other code as it is, even when its text starts the same way', () => {
    expect(refusalSentence({ code: ErrorCode.NETWORK_ERROR, message: 'Network error: the server is away.' })).toBe('Network error: the server is away.');
    expect(refusalSentence({ code: ErrorCode.NETWORK_ERROR, message: 'Invalid input: not a refusal.' })).toBe('Invalid input: not a refusal.');
  });

  it('leaves a message with no prefix, or no code, as it is', () => {
    expect(refusalSentence({ code: ErrorCode.INVALID_INPUT, message: 'The text is too long.' })).toBe('The text is too long.');
    expect(refusalSentence({ message: 'Invalid input: from nowhere.' })).toBe('Invalid input: from nowhere.');
  });

  it('drops only the one prefix', () => {
    expect(refusalSentence({ code: ErrorCode.INVALID_INPUT, message: 'Invalid input: Invalid input: twice.' })).toBe('Invalid input: twice.');
  });
});
