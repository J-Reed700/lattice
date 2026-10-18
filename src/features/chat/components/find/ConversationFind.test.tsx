import { useRef, type ReactNode } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { isMacOS } from '@/config/shortcuts';
import type { ConversationMessage } from '@/types/conversation';

import { ConversationFind } from './ConversationFind';
import { findInMessages, MAX_FIND_RESULTS, occurrenceAt, occurrenceRows } from './findInMessages';

const scrollToMessage = vi.fn((_id: string) => () => {});
vi.mock('@/utils/chatMessageNavigation', async importOriginal => ({
  ...await importOriginal<typeof import('@/utils/chatMessageNavigation')>(),
  scrollToMessage: (id: string) => scrollToMessage(id),
}));

const message = (id: string, role: string, content: string): ConversationMessage => ({
  id, conversationId: 'c1', role, content, tokens: 0, createdAt: '2026-10-17T10:00:00Z', status: 'complete' as ConversationMessage['status'],
});

const messages = [
  message('m1', 'user', 'How do Tangents work?'),
  message('m2', 'assistant', 'A tangent opens beside the\nconversation. Tangents keep their passage.'),
  message('m3', 'system', 'tangent instructions'),
  message('m4', 'assistant', 'Nothing relevant.'),
];

describe('findInMessages', () => {
  it('matches user and assistant content case-insensitively, across line breaks', () => {
    const { matches, truncated } = findInMessages(messages, 'TANGENT');
    expect(truncated).toBe(false);
    expect(matches.map(match => [match.message.id, match.count])).toEqual([['m1', 1], ['m2', 2]]);
    expect(matches[1].snippet).toMatchObject({ before: 'A ', match: 'tangent' });
    expect(findInMessages(messages, 'beside the conversation').matches.map(match => match.message.id)).toEqual(['m2']);
  });

  it('stops at the cap and says so', () => {
    const many = Array.from({ length: MAX_FIND_RESULTS + 1 }, (_, at) => message(`n${at}`, 'user', 'needle'));
    const { matches, truncated } = findInMessages(many, 'needle');
    expect(matches).toHaveLength(MAX_FIND_RESULTS);
    expect(truncated).toBe(true);
  });
});

describe('occurrenceRows', () => {
  const { matches } = findInMessages(messages, 'tangent');

  it('lays every occurrence out in conversation order', () => {
    const { rows, total } = occurrenceRows(matches, new Map());
    expect(total).toBe(3);
    expect(rows.map(row => [row.message.id, row.start, row.count])).toEqual([['m1', 0, 1], ['m2', 1, 2]]);
    expect([0, 1, 2].map(at => { const found = occurrenceAt(rows, at); return [found?.row.message.id, found?.ordinal]; }))
      .toEqual([['m1', 0], ['m2', 0], ['m2', 1]]);
    expect(occurrenceAt(rows, 3)).toBeNull();
  });

  it('takes the rendered count for a rendered message, and drops one that shows no match', () => {
    const content = (id: string) => messages.find(item => item.id === id)!.content;
    expect(occurrenceRows(matches, new Map([['m2', { content: content('m2'), count: 1 }]])).total).toBe(2);
    const { rows } = occurrenceRows(matches, new Map([['m1', { content: content('m1'), count: 0 }]]));
    expect(rows.map(row => [row.message.id, row.start])).toEqual([['m2', 0]]);
    // A count taken before the message changed no longer applies.
    expect(occurrenceRows(matches, new Map([['m2', { content: 'older text', count: 5 }]])).total).toBe(3);
  });
});

function Harness({ list = messages, children }: { list?: ConversationMessage[]; children?: ReactNode }) {
  const scopeRef = useRef<HTMLDivElement>(null);
  return <TooltipProvider>
    <ConversationFind messages={list} scopeRef={scopeRef} />
    <div ref={scopeRef}><textarea aria-label="Composer" />{children}</div>
  </TooltipProvider>;
}

describe('ConversationFind', () => {
  beforeEach(() => { vi.useFakeTimers(); scrollToMessage.mockClear(); });
  afterEach(() => { vi.useRealTimers(); });

  const type = (value: string) => {
    fireEvent.change(screen.getByRole('searchbox', { name: 'Find in conversation' }), { target: { value } });
    act(() => { vi.advanceTimersByTime(300); });
  };

  it('opens on Cmd/Ctrl+F from inside its chat, lists matches and jumps to one', () => {
    render(<Harness />);
    screen.getByRole('textbox', { name: 'Composer' }).focus();
    fireEvent.keyDown(document.activeElement!, { key: 'f', ...(isMacOS() ? { metaKey: true } : { ctrlKey: true }) });
    const box = screen.getByRole('searchbox', { name: 'Find in conversation' });
    expect(box).toHaveFocus();

    type('tangent');
    const results = screen.getByLabelText('Find results');
    expect(results.querySelectorAll('button')).toHaveLength(2);
    expect(results.querySelectorAll('mark')[0]).toHaveTextContent('Tangent');
    expect(screen.getByText(/Assistant ·.*2 matches/)).toBeInTheDocument();

    fireEvent.click(results.querySelectorAll('button')[1]);
    expect(scrollToMessage).toHaveBeenCalledWith('m2');
    expect(screen.queryByLabelText('Find results')).not.toBeInTheDocument();
    expect(screen.getByText('2/3')).toBeInTheDocument();
  });

  it('counts and steps occurrence by occurrence, within a message and wrapping at both ends', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: 'Find in conversation' }));
    type('tangent');
    const box = screen.getByRole('searchbox');
    expect(screen.getByText('–/3')).toBeInTheDocument();

    const steps: [boolean, string, string][] = [
      [false, '1/3', 'm1'], [false, '2/3', 'm2'], [false, '3/3', 'm2'], [false, '1/3', 'm1'],
      [true, '3/3', 'm2'], [true, '2/3', 'm2'],
    ];
    for (const [back, counter, id] of steps) {
      fireEvent.keyDown(box, { key: 'Enter', shiftKey: back });
      expect(screen.getByText(counter)).toBeInTheDocument();
      expect(scrollToMessage).toHaveBeenLastCalledWith(id);
    }
  });

  it('counts what a rendered message shows and moves within it without jumping to it again', () => {
    const list = [message('m5', 'assistant', 'Read [the issue](https://example.com/issue): `|issue| issue.exhausted`')];
    render(<Harness list={list}>
      <article id="message-m5"><div className="tiptap-viewer"><p>Read <a href="https://example.com/issue">the issue</a>: <code>|issue| issue.exhausted</code></p></div></article>
    </Harness>);
    fireEvent.click(screen.getByRole('button', { name: 'Find in conversation' }));
    type('issue');
    // The source has four; the URL is not on screen.
    expect(screen.getByText('–/3')).toBeInTheDocument();
    expect(screen.getByText(/3 matches/)).toBeInTheDocument();

    const box = screen.getByRole('searchbox');
    fireEvent.keyDown(box, { key: 'Enter' });
    expect(scrollToMessage).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(box, { key: 'Enter' });
    fireEvent.keyDown(box, { key: 'Enter' });
    expect(screen.getByText('3/3')).toBeInTheDocument();
    expect(scrollToMessage).toHaveBeenCalledTimes(1);
  });

  it('shows a floor, not an exact total, when the search stops at the cap', () => {
    const many = Array.from({ length: MAX_FIND_RESULTS + 1 }, (_, at) => message(`n${at}`, 'user', 'needle and needle'));
    render(<Harness list={many} />);
    fireEvent.click(screen.getByRole('button', { name: 'Find in conversation' }));
    type('needle');
    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Enter' });
    expect(screen.getByText(`1/${MAX_FIND_RESULTS * 2}+`)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('searchbox'));
    expect(screen.getByText(/Only the first 200 matching messages/)).toBeInTheDocument();
  });

  it('Escape clears and closes the search', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: 'Find in conversation' }));
    type('nothing');
    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Escape' });
    expect(screen.queryByRole('searchbox')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Find in conversation' }));
    expect(screen.getByRole('searchbox')).toHaveValue('');
  });
});
