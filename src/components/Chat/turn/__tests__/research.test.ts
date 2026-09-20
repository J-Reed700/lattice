import { describe, expect, it } from 'vitest';

import type { TurnStep } from '@/types/conversation';

import { deckOf, hasWebResearch, roundLine, type DeckRound, type DeckThinking } from '../research';

const step = (overrides: Partial<TurnStep> & Pick<TurnStep, 'id' | 'kind'>): TurnStep => ({
  label: overrides.id,
  state: 'done',
  startedAtMs: 0,
  ...overrides,
});

const search = (id: string, query: string, urls: string[], state: TurnStep['state'] = 'done') =>
  step({
    id,
    kind: 'web_search',
    detail: query,
    state,
    links: urls.map(url => ({ url, title: `Title of ${url}` })),
  });

const read = (id: string, url: string, state: TurnStep['state'], result?: string) =>
  step({ id, kind: 'read_page', state, result, links: [{ url }] });

const rounds = (steps: TurnStep[]) =>
  deckOf(steps).items.filter((item): item is DeckRound => item.type === 'round');

describe('research deck', () => {
  it('has nothing to add to a turn that never went to the web', () => {
    const steps = [step({ id: 'kb', kind: 'search_documents' }), step({ id: 'w', kind: 'generate' })];
    expect(hasWebResearch(steps)).toBe(false);
  });

  it('gives every page a search listed a fate: found, reading, read, or refused', () => {
    const [round] = rounds([
      search('s', 'local llm benchmarks', [
        'https://a.example/1',
        'https://b.example/2',
        'https://c.example/3',
        'https://d.example/4',
      ]),
      read('p1', 'https://a.example/1', 'done', '1,204 words'),
      read('p2', 'https://b.example/2', 'failed', '403 Forbidden'),
      read('p3', 'https://c.example/3', 'running'),
    ]);

    expect(round.pages.map(page => [page.host, page.state, page.note])).toEqual([
      ['a.example', 'read', '1,204 words'],
      ['b.example', 'failed', '403 Forbidden'],
      ['c.example', 'reading', null],
      ['d.example', 'found', null],
    ]);
    // The read step has no title of its own until it ends; the search's stays.
    expect(round.pages[2].title).toBe('Title of https://c.example/3');
    expect(round.active).toBe(true);
  });

  /** A trailing slash or a `www.` is the same page, not a second one. */
  it('files a read under the page the search listed, however the address was written', () => {
    const [round] = rounds([
      search('s', 'q', ['https://www.a.example/post/']),
      read('p', 'https://a.example/post', 'done', '300 words'),
    ]);
    expect(round.pages).toHaveLength(1);
    expect(round.pages[0].state).toBe('read');
  });

  it('does not knock a page back to found when a later search lists it again', () => {
    const [round] = rounds([
      search('s1', 'q', ['https://a.example/1']),
      read('p', 'https://a.example/1', 'done', '300 words'),
      search('s2', 'q more', ['https://a.example/1']),
    ]);
    expect(round.pages[0].state).toBe('read');
    expect(round.queries.map(query => query.text)).toEqual(['q', 'q more']);
  });

  it('starts a new round when the model goes back, and says that it chose to', () => {
    const deck = deckOf([
      step({ id: 'mode', kind: 'deep_research' }),
      search('s1', 'first', ['https://a.example/1']),
      step({ id: 'think-1', kind: 'generate' }),
      search('s2', 'second', ['https://b.example/2']),
      step({ id: 'think-2', kind: 'generate' }),
      search('s3', 'third', ['https://c.example/3']),
      step({ id: 'write', kind: 'generate' }),
    ]);

    expect(deck.deepResearch).toBe(true);
    expect(deck.rounds).toBe(3);
    // The mode is a heading, not a row of work.
    expect(deck.items.some(item => item.type === 'step' && item.step.kind === 'deep_research')).toBe(
      false
    );
    const thinking = deck.items.filter((item): item is DeckThinking => item.type === 'thinking');
    expect(thinking.map(item => [item.step.id, item.wentBack])).toEqual([
      ['think-1', true],
      ['think-2', true],
      ['write', false],
    ]);
  });

  /** Thinking and then looking for the first time is not going *back*. */
  it('does not call the first trip a return', () => {
    const deck = deckOf([
      step({ id: 'think', kind: 'generate' }),
      search('s', 'q', ['https://a.example/1']),
      step({ id: 'write', kind: 'generate' }),
    ]);
    const [first] = deck.items.filter((item): item is DeckThinking => item.type === 'thinking');
    expect(first.wentBack).toBe(false);
  });

  it('keeps the reason a search failed', () => {
    const [round] = rounds([
      step({
        id: 's',
        kind: 'web_search',
        detail: 'q',
        state: 'failed',
        result: 'All web search providers failed',
      }),
    ]);
    expect(round.queries[0].failure).toBe('All web search providers failed');
  });

  it('counts the pages that refused, because they are why it went back', () => {
    const [round] = rounds([
      search('s', 'q', ['https://a.example/1', 'https://b.example/2', 'https://c.example/3']),
      read('p1', 'https://a.example/1', 'done'),
      read('p2', 'https://b.example/2', 'failed'),
    ]);
    expect(roundLine(round)).toBe('1 search · 1 of 2 pages read');
  });

  it('says found, not read, of a round that has only searched so far', () => {
    const [round] = rounds([search('s', 'q', ['https://a.example/1', 'https://b.example/2'])]);
    expect(roundLine(round)).toBe('1 search · 2 pages found');
  });
});
