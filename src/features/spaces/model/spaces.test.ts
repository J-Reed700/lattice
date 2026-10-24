import { describe, expect, it } from 'vitest';

import { buildSpaceToolPreferencesJson, normalizeHexColor, parseSpaceToolPreferences, uniqueName } from './spaces';

describe('normalizeHexColor', () => {
  it.each([
    [null, null],
    [undefined, null],
    ['', null],
    ['   ', null],
    ['#ABC', '#abc'],
    [' 3a7F00 ', '#3a7f00'],
    ['#abcd', null],
    ['olive', null],
  ])('%s → %s', (input, expected) => {
    expect(normalizeHexColor(input)).toBe(expected);
  });
});

describe('space tool preferences', () => {
  const off = { knowledgeBase: false, webSearch: false, deepResearchMode: false, enabledTools: [] };

  it('turns the web tools on with web search and round-trips', () => {
    const json = buildSpaceToolPreferencesJson({ knowledgeBase: true, webSearch: true, deepResearchMode: true });
    expect(parseSpaceToolPreferences(json)).toEqual({
      knowledgeBase: true, webSearch: true, deepResearchMode: true, enabledTools: ['web_search', 'fetch_url_content'],
    });
  });

  it('reads nothing, malformed JSON and wrong types as all off', () => {
    expect(parseSpaceToolPreferences(null)).toEqual(off);
    expect(parseSpaceToolPreferences('{not json')).toEqual(off);
    expect(parseSpaceToolPreferences(JSON.stringify({ knowledgeBase: 'yes', web_search: 1, deepResearchMode: null }))).toEqual(off);
  });

  it('reads snake_case keys and de-duplicates a stored tool list', () => {
    const raw = JSON.stringify({ knowledge_base: true, web_search: false, deep_research_mode: true, enabled_tools: [' wiki ', 'wiki', 7, ''] });
    expect(parseSpaceToolPreferences(raw)).toEqual({ knowledgeBase: true, webSearch: false, deepResearchMode: true, enabledTools: ['wiki'] });
  });

  it('prefers a camelCase tool list when present', () => {
    const raw = JSON.stringify({ webSearch: true, enabledTools: ['web_search'], enabled_tools: ['other'] });
    expect(parseSpaceToolPreferences(raw).enabledTools).toEqual(['web_search']);
  });
});

describe('uniqueName', () => {
  it('keeps a free name, trimmed', () => {
    expect(uniqueName('  Botany ', ['History'], 'Space')).toBe('Botany');
  });

  it('suffixes a taken name, ignoring case, past every used suffix', () => {
    expect(uniqueName('botany', ['Botany', 'BOTANY 2'], 'Space')).toBe('botany 3');
  });

  it('numbers a blank request after the existing names, skipping used numbers', () => {
    expect(uniqueName('', ['Cooking'], 'Space')).toBe('Space 2');
    expect(uniqueName(' ', ['Cooking', 'space 3'], 'Space')).toBe('Space 4');
  });
});
