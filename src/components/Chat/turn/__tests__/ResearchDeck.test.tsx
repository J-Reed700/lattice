import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { TurnStep } from '@/types/conversation';

import { ResearchDeck } from '../ResearchDeck';

const openExternalUrl = vi.hoisted(() => vi.fn(async () => true));
vi.mock('@/utils/openExternalUrl', () => ({ openExternalUrl }));

const step = (overrides: Partial<TurnStep> & Pick<TurnStep, 'id' | 'kind'>): TurnStep => ({
  label: 'Doing something',
  state: 'done',
  startedAtMs: 0,
  durationMs: 1200,
  ...overrides,
});

const search = (id: string, query: string, urls: string[]) =>
  step({
    id,
    kind: 'web_search',
    label: 'Searching the web',
    detail: query,
    links: urls.map(url => ({ url, title: `About ${new URL(url).hostname}` })),
  });

const twoRounds: TurnStep[] = [
  step({ id: 'mode', kind: 'deep_research', label: 'Deep research', result: 'searches wider' }),
  search('s1', 'first question', ['https://a.example/1', 'https://b.example/2']),
  step({ id: 'p1', kind: 'read_page', result: '812 words', links: [{ url: 'https://a.example/1' }] }),
  step({ id: 't1', kind: 'generate', label: 'Thinking', result: '2 tool calls' }),
  search('s2', 'a sharper question', ['https://c.example/3']),
  step({
    id: 'p3',
    kind: 'read_page',
    state: 'running',
    durationMs: undefined,
    links: [{ url: 'https://c.example/3' }],
  }),
];

describe('ResearchDeck', () => {
  beforeEach(() => openExternalUrl.mockClear());

  it('draws plain rows and no card for a turn that stayed off the web', () => {
    render(
      <ResearchDeck
        live={false}
        steps={[step({ id: 'kb', kind: 'search_documents', label: 'Searching your documents' })]}
      />
    );
    expect(screen.getByText('Searching your documents')).toBeInTheDocument();
    expect(screen.queryByRole('region')).not.toBeInTheDocument();
  });

  it('shows what was searched and every page that came back', () => {
    render(<ResearchDeck live steps={[search('s', 'local llm benchmarks', ['https://a.example/1'])]} />);

    const card = screen.getByRole('region', { name: 'Web search' });
    expect(within(card).getByText('local llm benchmarks')).toBeInTheDocument();
    expect(within(card).getByText('About a.example')).toBeInTheDocument();
    expect(within(card).getByText('found')).toBeInTheDocument();
  });

  /**
   * The point of the whole thing: the second trip is as visible as the first,
   * and it is said to be a decision rather than more of the same search.
   */
  it('announces that the model went back and opens a card for the new round', () => {
    render(<ResearchDeck live steps={twoRounds} />);

    expect(
      screen.getByText('Not enough yet — starting another round of deep research')
    ).toBeInTheDocument();
    // Why this turn is long, said once and not as a row of work.
    expect(screen.getByText('searches wider')).toBeInTheDocument();
    const second = screen.getByRole('region', { name: 'Round 2' });
    expect(within(second).getByText('a sharper question')).toBeInTheDocument();
    expect(within(second).getByText('reading')).toBeInTheDocument();
  });

  it('folds the earlier round to its one line and lets the reader reopen it', async () => {
    render(<ResearchDeck live steps={twoRounds} />);

    const first = screen.getByRole('region', { name: 'Round 1' });
    expect(within(first).getByText('1 search · 1 page read')).toBeInTheDocument();
    expect(within(first).queryByText('first question')).not.toBeInTheDocument();

    await userEvent.click(within(first).getByRole('button', { expanded: false }));

    expect(within(first).getByText('first question')).toBeInTheDocument();
    expect(within(first).getByText('812 words')).toBeInTheDocument();
  });

  it('says a refused page was refused, in words and with the reason', () => {
    render(
      <ResearchDeck
        live={false}
        steps={[
          search('s', 'q', ['https://a.example/1']),
          step({
            id: 'p',
            kind: 'read_page',
            state: 'failed',
            result: '403 Forbidden',
            links: [{ url: 'https://a.example/1' }],
          }),
        ]}
      />
    );
    expect(screen.getByText('403 Forbidden')).toBeInTheDocument();
  });

  it('opens a page in the browser when it is clicked', async () => {
    render(<ResearchDeck live={false} steps={[search('s', 'q', ['https://a.example/1'])]} />);

    await userEvent.click(screen.getByRole('button', { name: /About a\.example/ }));

    expect(openExternalUrl).toHaveBeenCalledWith('https://a.example/1');
  });

  it('shows a thought in progress as one, and a finished one as a plain row', () => {
    render(
      <ResearchDeck
        live
        steps={[
          step({ id: 't', kind: 'generate', label: 'Thinking', state: 'running', durationMs: undefined }),
        ]}
      />
    );
    expect(screen.getByText('Thinking…')).toBeInTheDocument();
  });
});
