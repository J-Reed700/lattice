import { describe, expect, it } from 'vitest';

import { isContact, isHttpUrl, keyServiceOf, statusLine } from './sourcesLabels';

const finder = (overrides: Record<string, unknown> = {}) => ({
  gate: 'ready',
  needs: { kind: 'none' },
  health: { state: 'ok', calls: 0, medianMs: null, lastError: null, until: null },
  ...overrides,
}) as never;

const keys = [{ service: 'brave', name: 'Brave Search' }] as never;

describe('sourcesLabels', () => {
  it('names the key service a finder uses', () => {
    expect(keyServiceOf(finder({ needs: { kind: 'requiredKey', service: 'brave' } }))).toBe('brave');
    expect(keyServiceOf(finder({ needs: { kind: 'optionalKey', service: 'exa' } }))).toBe('exa');
    expect(keyServiceOf(finder())).toBeNull();
  });

  describe('statusLine', () => {
    it.each([
      [{ gate: 'disabled' }, 'Off'],
      [{ gate: 'noKey', needs: { kind: 'requiredKey', service: 'brave' } }, 'Needs a Brave Search key'],
      [{ gate: 'noKey', needs: { kind: 'requiredKey', service: 'tavily' } }, 'Needs a tavily key'],
      [{ gate: 'noKey' }, 'Needs a key'],
      [{ gate: 'noInstance' }, 'Needs your SearXNG address'],
      [{ gate: 'noContact' }, 'Needs a contact'],
    ])('says why a finder is off: %j', (overrides, text) => {
      expect(statusLine(finder(overrides), keys)).toEqual({ text, tone: 'muted' });
    });

    it('warns about a paused or retrying finder', () => {
      const until = '2026-05-15T14:30:00Z';
      const paused = statusLine(finder({ health: { state: 'coolingDown', until, lastError: 'HTTP 429', calls: 3, medianMs: 1 } }), keys);
      expect(paused.tone).toBe('warning');
      expect(paused.text).toMatch(/^Paused until .+ \(HTTP 429\)$/);
      expect(statusLine(finder({ health: { state: 'coolingDown', until: 'soon', lastError: null } }), keys).text).toBe('Paused until soon');
      expect(statusLine(finder({ health: { state: 'coolingDown', until: null, lastError: null } }), keys).text).toBe('Paused');
      expect(statusLine(finder({ health: { state: 'retrying' } }), keys)).toEqual({ text: 'Trying again after a pause', tone: 'warning' });
    });

    it('reports a healthy finder’s record', () => {
      expect(statusLine(finder(), keys)).toEqual({ text: 'Ready', tone: 'ok' });
      expect(statusLine(finder({ health: { state: 'ok', calls: 1, medianMs: null, lastError: null } }), keys).text).toBe('Healthy · 1 call');
      expect(statusLine(finder({ health: { state: 'ok', calls: 12, medianMs: 2345, lastError: 'timeout' } }), keys).text)
        .toBe('Healthy · 12 calls · median 2.3 s · last error: timeout');
    });
  });

  it.each([
    ['https://search.example.org', true],
    ['http://localhost:8080', true],
    ['ftp://files.example.org', false],
    ['not a url', false],
  ])('checks %s is an http address: %s', (value, expected) => {
    expect(isHttpUrl(value)).toBe(expected);
  });

  it.each([
    ['librarian@museum.org', true],
    ['https://museum.org/contact', true],
    ['', false],
    ['has space@museum.org', false],
    ['@museum.org', false],
    ['a@b@museum.org', false],
    ['curator@localhost', false],
    ['curator@museum..org', false],
    ['museum.org', false],
  ])('checks %j is a contact: %s', (value, expected) => {
    expect(isContact(value)).toBe(expected);
  });
});
