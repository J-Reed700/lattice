import { describe, expect, it } from 'vitest';

import { getUserFriendlyMessage, redact, redactSecrets, sanitizeErrorMessage } from '../redact';
import { SANITIZED_MESSAGE, SENSITIVE_MESSAGE } from './sensitiveMessage';

const OPAQUE_KEY = `key-${'Zq8x'.repeat(11)}`;

const UNCHANGED = [
  'Error: something went wrong',
  "TypeError: Cannot read properties of undefined (reading 'x')",
  'Failed to fetch: network down',
  'read/write failed and/or timed out at 1/2',
  'N/A',
  'TCP/IP',
  'The index is empty',
  '',
];

const STRIPPED: Array<[string, string, string]> = [
  ['Windows path, backslashes', 'Cannot open C:\\Users\\bob\\notes.txt', 'Cannot open [PATH]'],
  ['Windows path, slashes', 'Cannot open C:/Users/bob/notes.txt', 'Cannot open [PATH]'],
  [
    'Windows path with accents and spaces',
    'C:\\Users\\José\\Lattice Compatibility café t403\\app.exe failed',
    '[PATH]',
  ],
  ['UNC path', 'Cannot open \\\\fileserver\\share\\notes.txt', 'Cannot open [PATH]'],
  ['Unix path with a space', '/home/bob/my notes.txt: denied', '[PATH]: denied'],
  ['home-relative path', '~/notes/a.txt', '[PATH]'],
  ['relative path, slashes', 'home/bob/notes.txt', '[PATH]'],
  ['relative path after a colon', 'Failed:home/bob/notes.txt', 'Failed:[PATH]'],
  ['relative backslash path after a semicolon', 'path;Users\\bob\\x.txt', 'path;[PATH]'],
  ['log-redacted Unix path', 'Cannot open /[USER]/private/a.txt', 'Cannot open [PATH]'],
  ['log-redacted Windows path', 'Cannot open [USER]\\secret.txt', 'Cannot open [PATH]'],
  ['relative path, backslashes', 'Users\\bob\\notes.txt', '[PATH]'],
  ['URL with host, port and query', 'https://private.example.net:8089/v1?x=1', '[URL]'],
  ['URL with an IP', 'http://192.168.1.4:8089/v1', '[URL]'],
  ['email', 'Sent to bob@example.com failed', 'Sent to [EMAIL] failed'],
  ['Unicode email', 'Mail to zoë@exämple.org failed', 'Mail to [EMAIL] failed'],
  ['IPv4', 'connect to 10.0.0.5 refused', 'connect to [IP] refused'],
  [
    '32-character hex',
    'key 0123456789abcdef0123456789abcdef rejected',
    'key [TOKEN] rejected',
  ],
  ['48-character opaque key', `Rejected ${OPAQUE_KEY} today`, 'Rejected [TOKEN] today'],
  ['combined', SENSITIVE_MESSAGE, SANITIZED_MESSAGE],
];

describe('sanitizeErrorMessage', () => {
  it.each(UNCHANGED)('leaves %j as it is', (text) => {
    expect(sanitizeErrorMessage(text)).toBe(text);
  });

  it.each(STRIPPED)('strips %s', (_name, input, expected) => {
    expect(sanitizeErrorMessage(input)).toBe(expected);
  });

  it('keeps no part of an accented path', () => {
    const sanitized = sanitizeErrorMessage(
      'C:\\Users\\José\\Lattice Compatibility café t403\\app.exe failed'
    );
    expect(sanitized).not.toContain('José');
    expect(sanitized).not.toContain('café');
  });

  it('removes the secret from an Authorization header', () => {
    const sanitized = sanitizeErrorMessage('Authorization: Bearer abc123');
    expect(sanitized).not.toContain('abc123');
    expect(sanitized).toContain('[REDACTED]');
  });

  it('removes the secret from an api_key assignment', () => {
    expect(sanitizeErrorMessage('api_key=abc123')).toBe('api_key=[REDACTED]');
  });

  it.each([
    ...UNCHANGED,
    ...STRIPPED.map(([, input]) => input),
    'Authorization: Bearer abc123',
    'api_key=abc123',
  ])(
    'gives the same result when %j is sanitized twice',
    (text) => {
      const once = sanitizeErrorMessage(text);
      expect(sanitizeErrorMessage(once)).toBe(once);
    }
  );

  it.each([undefined, null, 42, {}, true])('returns an empty string for %j', (value) => {
    expect(() => sanitizeErrorMessage(value)).not.toThrow();
    expect(sanitizeErrorMessage(value)).toBe('');
  });

  it('finishes quickly on a very long run with nothing to strip', () => {
    const started = Date.now();
    sanitizeErrorMessage('a'.repeat(200_000));
    sanitizeErrorMessage('a, '.repeat(50_000));
    expect(Date.now() - started).toBeLessThan(5000);
  });
});

describe('getUserFriendlyMessage', () => {
  const fallback = 'Something unexpected happened.';

  it('sanitizes the message of an error', () => {
    expect(getUserFriendlyMessage(new Error(SENSITIVE_MESSAGE))).toBe(SANITIZED_MESSAGE);
  });

  it('uses a thrown string, sanitized', () => {
    expect(getUserFriendlyMessage('boom')).toBe('boom');
    expect(getUserFriendlyMessage('Sent to bob@example.com failed')).toBe('Sent to [EMAIL] failed');
  });

  it('falls back when there is no message', () => {
    const noMessage = new Error('x');
    Object.defineProperty(noMessage, 'message', { value: undefined });
    expect(getUserFriendlyMessage(noMessage)).toBe(fallback);
    expect(getUserFriendlyMessage(null)).toBe(fallback);
    expect(getUserFriendlyMessage(undefined)).toBe(fallback);
    expect(getUserFriendlyMessage({})).toBe(fallback);
    expect(getUserFriendlyMessage(42)).toBe(fallback);
    expect(getUserFriendlyMessage(new Error('   '))).toBe(fallback);
    expect(getUserFriendlyMessage(new Error(''))).toBe(fallback);
  });

  it('does not throw when reading the message throws', () => {
    const hostile = {
      get message(): string {
        throw new Error('no');
      },
    };
    expect(getUserFriendlyMessage(hostile)).toBe(fallback);
  });
});

describe('redactSecrets', () => {
  it('redacts JSON secret fields', () => {
    expect(redactSecrets('{"token":"supersecret"}')).toBe('{"token":"[REDACTED]"}');
  });

  it('redacts a Bearer token', () => {
    expect(redactSecrets('Bearer abc123')).toBe('Bearer [REDACTED]');
  });

  it('redacts key=value secrets', () => {
    expect(redactSecrets('password=hunter2')).toBe('password=[REDACTED]');
  });

  it('redacts secret query parameters', () => {
    expect(redactSecrets('/v1?key=abc#frag')).toBe('/v1?key=[REDACTED]#frag');
  });

  it('leaves user folders alone', () => {
    expect(redactSecrets('/Users/bob/x')).toBe('/Users/bob/x');
  });
});

describe('redact', () => {
  it('applies the secret rules', () => {
    expect(redact('{"token":"supersecret"} Bearer abc123')).toBe(
      '{"token":"[REDACTED]"} Bearer [REDACTED]'
    );
  });

  it('hides the user folder on Unix and Windows', () => {
    expect(redact('/Users/bob/private')).toBe('/[USER]/private');
    expect(redact('/home/bob/private')).toBe('/[USER]/private');
    expect(redact('C:\\Users\\bob\\private')).toBe('[USER]\\private');
  });

  it('leaves other text and paths alone', () => {
    expect(redact('Failed to fetch: network down')).toBe('Failed to fetch: network down');
    expect(redact('/var/log/app.log')).toBe('/var/log/app.log');
  });
});
