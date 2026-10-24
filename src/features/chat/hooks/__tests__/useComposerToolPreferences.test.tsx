import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DEEP_RESEARCH_WARNING_MESSAGE, type TurnMode } from '@/features/chat/components/ComposerControls';
import { useComposerToolPreferences } from '@/features/chat/hooks/useComposerToolPreferences';

const toast = vi.hoisted(() => ({ warning: vi.fn() }));
vi.mock('@/stores/toastStore', () => ({ toast }));

type Options = Parameters<typeof useComposerToolPreferences>[0];

const conversation = (id: string, spaceId: string | null) => ({ id, spaceId }) as never;
const space = (id: string, preferences: Record<string, unknown> | null) =>
  ({ id, toolPreferencesJson: preferences ? JSON.stringify(preferences) : null }) as never;
const customTools = [{ name: 'sourdough_timer' }, { name: 'ferment_log' }] as Options['customTools'];
const spaceDoc = (documentId: string, title: string) => ({ documentId, title }) as never;

const baseOptions = (): Options => ({
  conversationId: 'c-bread',
  conversations: [conversation('c-bread', 'space-baking'), conversation('c-loose', null)],
  spaces: [space('space-baking', { knowledgeBase: true, webSearch: true, enabledTools: ['web_search', 'wiki_search', 'sourdough_timer'] })],
  customTools,
});

const renderPrefs = (overrides: Partial<Options> = {}) =>
  renderHook((props: Options) => useComposerToolPreferences(props), {
    initialProps: { ...baseOptions(), ...overrides },
  });

beforeEach(() => vi.clearAllMocks());

describe('loading a conversation', () => {
  it('starts from the defaults without a conversation', () => {
    const { result } = renderPrefs({ conversationId: null });
    expect(result.current.toolPreferences).toEqual({
      knowledgeBase: false, webSearch: false, deepResearchMode: false, followupMode: false, turnMode: 'auto', enabledTools: [],
      focusDocumentIds: [],
    });
    expect(result.current.turnMode).toBe('auto');
    expect(result.current.wikipediaEnabled).toBe(false);
    expect(result.current.enabledCustomToolNames).toEqual([]);
  });

  it("applies the conversation's space switches when the chat opens", () => {
    const { result } = renderPrefs();
    expect(result.current.toolPreferences).toMatchObject({ knowledgeBase: true, webSearch: true });
    expect(result.current.wikipediaEnabled).toBe(true);
    expect(result.current.enabledCustomToolNames).toEqual(['sourdough_timer']);
    expect([...result.current.enabledToolSet]).toEqual(['web_search', 'wiki_search', 'sourdough_timer']);
  });

  it('uses the defaults for a conversation outside any space, and again once the chat closes', () => {
    const { result, rerender } = renderPrefs();
    rerender({ ...baseOptions(), conversationId: 'c-loose' });
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);

    rerender({ ...baseOptions(), conversationId: 'c-bread' });
    expect(result.current.toolPreferences.knowledgeBase).toBe(true);
    rerender({ ...baseOptions(), conversationId: null });
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);
  });

  it('waits for the conversation and its space to load before applying anything', () => {
    const { result, rerender } = renderPrefs({ conversations: [], spaces: [] });
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);

    rerender({ ...baseOptions(), spaces: [] });
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);

    rerender(baseOptions());
    expect(result.current.toolPreferences.knowledgeBase).toBe(true);
  });

  it('keeps switches flipped in this chat when the conversation list refreshes', () => {
    const { result, rerender } = renderPrefs();
    act(() => result.current.toggleKnowledgeBase());
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);

    rerender(baseOptions());
    expect(result.current.toolPreferences.knowledgeBase).toBe(false);
  });
});

describe('switches', () => {
  it('turns the web tools on and off together with web search', () => {
    const { result } = renderPrefs({ conversationId: null });
    act(() => result.current.toggleWebTools());
    expect(result.current.toolPreferences).toMatchObject({ webSearch: true, enabledTools: ['web_search', 'fetch_url_content'] });

    act(() => result.current.toggleWebTools());
    expect(result.current.toolPreferences).toMatchObject({ webSearch: false, enabledTools: [] });
  });

  it('turns both Wikipedia tools on, and off when either is on', () => {
    const { result } = renderPrefs({ conversationId: null });
    act(() => result.current.toggleWikiTools());
    expect(result.current.toolPreferences.enabledTools).toEqual(['wiki_search', 'wiki_summary']);
    expect(result.current.wikipediaEnabled).toBe(true);

    const { result: partial } = renderPrefs();
    act(() => partial.current.toggleWikiTools());
    expect(partial.current.toolPreferences.enabledTools).toEqual(['web_search', 'sourdough_timer']);
    expect(partial.current.wikipediaEnabled).toBe(false);
  });

  it('flips one custom tool at a time', () => {
    const { result } = renderPrefs();
    act(() => result.current.toggleCustomTool('ferment_log'));
    expect(result.current.enabledCustomToolNames).toEqual(['sourdough_timer', 'ferment_log']);

    act(() => result.current.toggleCustomTool('sourdough_timer'));
    expect(result.current.enabledCustomToolNames).toEqual(['ferment_log']);
  });

  it('warns only when deep research is switched on', () => {
    const { result } = renderPrefs({ conversationId: null });
    act(() => result.current.toggleDeepResearch());
    expect(result.current.toolPreferences.deepResearchMode).toBe(true);
    expect(toast.warning).toHaveBeenCalledWith('Deep research enabled', { message: DEEP_RESEARCH_WARNING_MESSAGE, duration: 5000 });

    act(() => result.current.toggleDeepResearch());
    expect(result.current.toolPreferences.deepResearchMode).toBe(false);
    expect(toast.warning).toHaveBeenCalledTimes(1);
  });

  it('records the turn mode and whether it follows up', () => {
    const { result } = renderPrefs({ conversationId: null });
    act(() => result.current.handleTurnModeChange('followup'));
    expect(result.current.toolPreferences).toMatchObject({ turnMode: 'followup', followupMode: true });
    expect(result.current.turnMode).toBe('followup');

    act(() => result.current.handleTurnModeChange('unknown' as TurnMode));
    expect(result.current.turnMode).toBe('auto');
    expect(result.current.resolveEffectiveToolPreferences().turnMode).toBe('auto');
  });
});

describe('what a turn goes out with', () => {
  it('turns every source on in query mode, without repeating a tool', () => {
    const { result } = renderPrefs();
    act(() => result.current.toggleKnowledgeBase());
    act(() => result.current.toggleWebTools());
    act(() => result.current.handleTurnModeChange('query'));

    expect(result.current.resolveEffectiveToolPreferences()).toMatchObject({
      turnMode: 'query',
      followupMode: false,
      knowledgeBase: true,
      webSearch: true,
      enabledTools: ['wiki_search', 'sourdough_timer', 'web_search', 'fetch_url_content', 'ferment_log'],
    });
  });

  it('sends the switches as set outside query mode', () => {
    const { result } = renderPrefs();
    act(() => result.current.handleTurnModeChange('followup'));

    expect(result.current.resolveEffectiveToolPreferences()).toMatchObject({
      turnMode: 'followup',
      followupMode: true,
      knowledgeBase: true,
      webSearch: true,
      enabledTools: ['web_search', 'wiki_search', 'sourdough_timer'],
    });
  });

  it('pins the turn to the focus documents until another chat opens', () => {
    const { result, rerender } = renderPrefs();
    const starters = [spaceDoc('d-levain', 'Levain schedules'), spaceDoc('d-crumb', 'Crumb structure')];
    act(() => result.current.applyFocusDocuments(starters));
    expect(result.current.focusDocuments).toEqual(starters);
    expect(result.current.resolveEffectiveToolPreferences().focusDocumentIds).toEqual(['d-levain', 'd-crumb']);

    rerender({ ...baseOptions(), conversationId: 'c-loose' });
    expect(result.current.focusDocuments).toEqual([]);
    expect(result.current.resolveEffectiveToolPreferences().focusDocumentIds).toEqual([]);
  });
});
