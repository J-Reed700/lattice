import { describe, expect, it } from 'vitest';

import {
  AuthenticationError,
  ConfigurationError,
  DatabaseError,
  FileSystemError,
  getUserFriendlyMessage,
  NetworkError,
  sanitizeErrorMessage,
  SearchError,
} from '../errors';

describe('sanitizeErrorMessage', () => {
  it('replaces an email address', () => {
    expect(sanitizeErrorMessage('Sent to bob@example.com failed')).toBe('Sent to [EMAIL] failed');
  });

  it('replaces an IPv4 address', () => {
    expect(sanitizeErrorMessage('connect to 10.0.0.5 refused')).toBe('connect to [IP] refused');
  });

  it('replaces a hex token of 32 characters or more', () => {
    expect(sanitizeErrorMessage('key 0123456789abcdef0123456789abcdef rejected')).toBe('key [TOKEN] rejected');
  });

  it('replaces a Windows path', () => {
    expect(sanitizeErrorMessage('Cannot open C:\\Users\\bob\\notes.txt')).toBe('Cannot open [PATH]');
  });

  it('replaces a Unix path', () => {
    const sanitized = sanitizeErrorMessage('Cannot open /home/bob/notes.txt');
    expect(sanitized).toContain('[PATH]');
    expect(sanitized).not.toContain('bob');
    expect(sanitized).not.toContain('notes.txt');
  });

  it('leaves a message with nothing sensitive as it is', () => {
    expect(sanitizeErrorMessage('The index is empty')).toBe('The index is empty');
  });
});

describe('getUserFriendlyMessage', () => {
  it('names each custom error type in plain words', () => {
    expect(getUserFriendlyMessage(new NetworkError('x'))).toBe("Couldn't reach the service. Check your connection.");
    expect(getUserFriendlyMessage(new AuthenticationError())).toBe('Sign in to continue.');
    expect(getUserFriendlyMessage(new DatabaseError('x'))).toBe("The database couldn't complete that. Your data is intact.");
    expect(getUserFriendlyMessage(new FileSystemError('x'))).toBe("Couldn't read or write the file. Check its permissions.");
    expect(getUserFriendlyMessage(new SearchError('x'))).toBe('Search failed. Try a different query.');
    expect(getUserFriendlyMessage(new ConfigurationError('x'))).toBe('A setting is invalid. Check Settings.');
  });

  it('does not repeat the message of a custom error', () => {
    const message = getUserFriendlyMessage(new FileSystemError('EACCES C:\\Users\\bob\\notes.txt'));
    expect(message).not.toContain('bob');
  });

  it('sanitizes the message of an unknown error', () => {
    expect(getUserFriendlyMessage(new Error('Sent to bob@example.com failed'))).toBe('Sent to [EMAIL] failed');
  });

  it('falls back to a generic line when the message is blank', () => {
    expect(getUserFriendlyMessage(new Error('   '))).toBe('Something unexpected happened.');
  });
});
