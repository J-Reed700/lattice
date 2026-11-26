import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, expect, it, vi } from 'vitest';

import { InsightMessage } from '@/features/journal/components/InsightMessage';
import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { reply } from '@/tests/replyFixtures';
import type { ReplyPieceDto, SupportDto } from '@/lib/bindings';
import type { SnapshotMessage } from '@/types/api/dailyNotes';
import { withoutMarks } from '@/utils/replyPieces';

vi.mock('@/features/reading/components/CitationHoverCard', async () => await vi.importActual('@/features/reading/components/CitationHoverCard'));

beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  citationDisplayStore.getState().setVisible(true);
});

const cited = (number: number, support: SupportDto): ReplyPieceDto => ({
  type: 'reference',
  reference: { written: `[s${number}]`, kind: 'source', number, support },
});

it('opens and hides sparse original citations without treating other bracketed numbers as sources', async () => {
  const onOpenSource = vi.fn();
  const message: SnapshotMessage = {
    id: 'message', role: 'assistant', createdAt: '2026-10-08T12:00:00Z',
    content: 'Read the saved passage [s7]. Step [1] is ordinary text.',
    reply: reply('Read the saved passage ', 7, '. Step [1] is ordinary text.'),
    metadata: JSON.stringify({ sources: [{ citationId: 7, documentId: 'document', filePath: '/docs/source.md', fileName: 'Saved source' }] }),
  };
  const { container } = render(<MemoryRouter><InsightMessage message={message} onOpenSource={onOpenSource} /></MemoryRouter>);

  const chip = await screen.findByRole('button', { name: 'Citation 7' });
  fireEvent.click(chip);
  expect(onOpenSource).toHaveBeenCalledWith(expect.objectContaining({ citationId: 7, documentId: 'document' }), 0);
  expect(screen.getByRole('button', { name: 'Citation 7: Saved source' })).toBeVisible();
  expect(screen.getByText('Saved source')).toBeVisible();

  act(() => citationDisplayStore.getState().setVisible(false));
  await waitFor(() => expect(container.querySelector('.citation-hidden')).not.toBeNull());
  expect(screen.queryByRole('button', { name: 'Citation 7' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Citation 7: Saved source' })).not.toBeInTheDocument();
  expect(container).toHaveTextContent('Step [1] is ordinary text.');
});

it('numbers strictly from backend citationIds without inventing positional fallbacks', async () => {
  const message: SnapshotMessage = {
    id: 'strict-ids',
    role: 'assistant',
    createdAt: '2026-10-08T12:00:00Z',
    content: 'First [s7].',
    reply: reply('First ', 7, '.'),
    metadata: JSON.stringify({
      sources: [
        { documentId: 'unnumbered', filePath: '/docs/unnumbered.md', fileName: 'Unnumbered source' },
        { citationId: 7, documentId: 'doc-7', filePath: '/docs/seven.md', fileName: 'Numbered source' },
      ],
    }),
  };

  render(<MemoryRouter><InsightMessage message={message} onOpenSource={vi.fn()} /></MemoryRouter>);

  await screen.findByRole('button', { name: 'Citation 7' });
  expect(screen.queryByRole('button', { name: /Citation 1/ })).not.toBeInTheDocument();
  expect(screen.queryByText('Unnumbered source')).not.toBeInTheDocument();
  expect(screen.getByText('Numbered source')).toBeVisible();
});

it('renders CitationCard support relations, Quoted marks, and passes anchored occurrences on open', async () => {
  const onOpenSource = vi.fn();
  const snapshotText = '# Guide\n\n## Mitigation\n\nHold the mitigation for 20 seconds.\n\n## Escalation\n\nPage Marisol Quenby once the hold expires.';
  const quoteStart = snapshotText.indexOf('Hold the mitigation for 20 seconds.');
  const quoteEnd = quoteStart + 'Hold the mitigation for 20 seconds.'.length;
  const pieces: ReplyPieceDto[] = [
    { type: 'quoted', text: 'Hold the mitigation for 20 seconds.' },
    { type: 'text', text: ' ' },
    cited(7, {
      relation: 'quotes',
      anchors: [{ exact: 'Hold the mitigation for 20 seconds.', span: { start: quoteStart, end: quoteEnd } }],
    }),
    { type: 'text', text: ' Notify Marisol after the timeout ' },
    cited(8, {
      relation: 'backs',
      anchors: [{ exact: 'Page Marisol Quenby once the hold expires.' }],
    }),
    { type: 'text', text: ' Pending note ' },
    cited(9, { relation: 'cites', anchors: [] }),
    { type: 'text', text: '.' },
  ];
  const message: SnapshotMessage = {
    id: 'msg-relations',
    role: 'assistant',
    createdAt: '2026-10-08T12:00:00Z',
    content: 'Hold the mitigation for 20 seconds. [s7] Notify Marisol after the timeout [s8] Pending note [s9].',
    reply: pieces,
    metadata: JSON.stringify({
      sources: [
        {
          citationId: 7,
          documentId: 'web:https://example.org/guide',
          filePath: 'https://example.org/guide',
          fileName: 'Operations Guide',
          category: 'Web article',
          content: 'Hold the mitigation for 20 seconds.',
          webSnapshot: { url: 'https://example.org/guide', title: 'Operations Guide', text: snapshotText, fetchedAt: null, truncated: false },
        },
        {
          citationId: 8,
          documentId: 'doc-8',
          filePath: '/vault/escalate.md',
          fileName: 'Escalation Runbook',
          content: 'Page Marisol Quenby once the hold expires.',
        },
        {
          citationId: 9,
          documentId: 'doc-9',
          filePath: '/vault/pending.md',
          fileName: 'Pending Doc',
          content: 'Unchecked passage.',
        },
      ],
    }),
  };

  const { container } = render(<MemoryRouter><InsightMessage message={message} onOpenSource={onOpenSource} /></MemoryRouter>);

  const chip7 = await screen.findByRole('button', { name: 'Citation 7' });
  expect(Array.from(container.querySelectorAll('.cite-quote')).map((el) => el.textContent)).toEqual(['“', '”']);
  expect(withoutMarks(container.textContent ?? '')).toContain('“Hold the mitigation for 20 seconds.”');

  fireEvent.mouseOver(chip7);
  const tooltip7 = await screen.findByRole('tooltip');
  expect(tooltip7).toHaveTextContent('Quoted from');
  expect(tooltip7).toHaveTextContent('“Hold the mitigation for 20 seconds.”');
  expect(tooltip7).toHaveTextContent('Guide › Mitigation');

  const chip8 = screen.getByRole('button', { name: 'Citation 8' });
  fireEvent.mouseOver(chip8);
  const tooltip8 = await screen.findByRole('tooltip');
  expect(tooltip8).toHaveTextContent('Supported by');
  expect(tooltip8).toHaveTextContent('Page Marisol Quenby once the hold expires.');

  const chip9 = screen.getByRole('button', { name: 'Citation 9' });
  fireEvent.mouseOver(chip9);
  const tooltip9 = await screen.findByRole('tooltip');
  expect(tooltip9).toHaveTextContent('Cited (not checked)');
  expect(tooltip9).toHaveTextContent('Unchecked passage.');

  fireEvent.click(chip7);
  expect(onOpenSource).toHaveBeenCalledWith(
    expect.objectContaining({
      citationId: 7,
      anchoredBy: [expect.objectContaining({ occurrence: 0, relation: 'quotes' })],
    }),
    0,
  );
});
