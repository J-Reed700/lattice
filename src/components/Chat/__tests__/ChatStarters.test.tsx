import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ChatStarters } from '../ChatStarters';

import type { ChatStarters as ChatStartersData } from '../../../types/api/chatStarters';

const queryResult = vi.hoisted(() => ({
  current: {
    data: undefined as ChatStartersData | undefined,
    isLoading: false,
  },
  askedFor: undefined as string | null | undefined,
}));

vi.mock('@/hooks/queries/useChatStartersQuery', () => ({
  useChatStartersQuery: (spaceId: string | null) => {
    queryResult.askedFor = spaceId;
    return queryResult.current;
  },
}));

const setData = (data: ChatStartersData | undefined, isLoading = false) => {
  queryResult.current = { data, isLoading };
};

describe('ChatStarters', () => {
  it('renders three rows and hands the question to onPick', async () => {
    setData({
      fingerprint: 'fp',
      generatedAt: '2026-09-06T00:00:00Z',
      documentCount: 1247,
      starters: [
        { question: 'What did I decide about pricing?' },
        { question: 'Which papers disagree about sample size?' },
        { question: 'What is still unresolved in the migration notes?' },
      ],
    });
    const onPick = vi.fn();
    render(<ChatStarters onPick={onPick} />);

    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    await userEvent.click(screen.getByText('What did I decide about pricing?'));
    expect(onPick).toHaveBeenCalledWith('What did I decide about pricing?');
  });

  it('renders the lead-in and no questions when the list is empty', () => {
    setData({
      fingerprint: 'fp',
      generatedAt: '2026-09-06T00:00:00Z',
      documentCount: 1247,
      starters: [],
    });
    render(<ChatStarters onPick={vi.fn()} />);

    expect(screen.getByText('Ask something about your 1,247 documents.')).toBeInTheDocument();
    expect(screen.queryAllByRole('listitem')).toHaveLength(0);
  });

  it('never invents a question from an empty payload', () => {
    setData({
      fingerprint: 'fp',
      generatedAt: '2026-09-06T00:00:00Z',
      documentCount: 42,
      starters: [],
    });
    const { container } = render(<ChatStarters onPick={vi.fn()} />);

    // The anti-fabrication guard: no "What do my 42 PDFs say about…" ever.
    expect(container.textContent).not.toContain('PDFs');
    expect(container.textContent).not.toContain('?');
  });

  it('renders nothing when nothing is indexed', () => {
    setData({
      fingerprint: 'empty',
      generatedAt: '2026-09-06T00:00:00Z',
      documentCount: 0,
      starters: [],
    });
    const { container } = render(<ChatStarters onPick={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing while loading', () => {
    setData(undefined, true);
    const { container } = render(<ChatStarters onPick={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  // The reported bug: a chat in one space opened with questions drawn from
  // another space's library, because the questions were never asked per space.
  it('asks for the questions of the space it was given', () => {
    setData(undefined, true);

    render(<ChatStarters onPick={vi.fn()} spaceId="movies" />);
    expect(queryResult.askedFor).toBe('movies');

    render(<ChatStarters onPick={vi.fn()} />);
    expect(queryResult.askedFor).toBeNull();
  });
});
