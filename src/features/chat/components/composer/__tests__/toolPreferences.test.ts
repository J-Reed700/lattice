import { describe, expect, it } from 'vitest';

import {
  defaultToolPreferences,
  parseToolPreferences,
} from '@/features/chat/components/composer/toolPreferences';

describe('toolPreferences', () => {
  it('defaults strictSources to false (hybrid mode)', () => {
    expect(defaultToolPreferences()).toMatchObject({
      knowledgeBase: false,
      webSearch: false,
      strictSources: false,
    });
  });

  it('parses strictSources from camelCase and snake_case with or without enabledTools', () => {
    expect(
      parseToolPreferences(JSON.stringify({ strictSources: true, enabledTools: ['web_search'] }))
    ).toMatchObject({
      strictSources: true,
      enabledTools: ['web_search'],
    });

    expect(
      parseToolPreferences(JSON.stringify({ strict_sources: true, knowledge_base: false }))
    ).toMatchObject({
      knowledgeBase: false,
      strictSources: true,
      enabledTools: [],
    });

    expect(parseToolPreferences(JSON.stringify({ knowledgeBase: true }))).toMatchObject({
      knowledgeBase: true,
      strictSources: false,
    });
  });
});
