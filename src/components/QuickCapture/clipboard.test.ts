import { describe, it, expect, vi, afterEach } from 'vitest';

import { clipboardUrl, readClipboardText } from './clipboard';

function stubClipboard(readText: () => Promise<string>) {
  Object.defineProperty(navigator, 'clipboard', {
    value: { readText },
    configurable: true,
    writable: true,
  });
}

describe('clipboardUrl', () => {
  it('accepts an https URL', () => {
    const url = clipboardUrl('https://example.com/a');
    expect(url?.host).toBe('example.com');
  });

  it('accepts an http URL', () => {
    expect(clipboardUrl('http://x.dev')).not.toBeNull();
  });

  it('rejects a non-http protocol', () => {
    expect(clipboardUrl('ftp://x')).toBeNull();
  });

  it('rejects a URL embedded in a sentence', () => {
    expect(clipboardUrl('look at https://x.dev')).toBeNull();
  });

  it('rejects empty and null input', () => {
    expect(clipboardUrl('')).toBeNull();
    expect(clipboardUrl(null)).toBeNull();
  });
});

describe('readClipboardText', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns null when the read throws', async () => {
    stubClipboard(() => Promise.reject(new Error('denied')));
    await expect(readClipboardText()).resolves.toBeNull();
  });

  it('returns null when the clipboard is blank', async () => {
    stubClipboard(() => Promise.resolve('   \n '));
    await expect(readClipboardText()).resolves.toBeNull();
  });

  it('trims the clipboard text', async () => {
    stubClipboard(() => Promise.resolve('  https://example.com  '));
    await expect(readClipboardText()).resolves.toBe('https://example.com');
  });
});
