import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ClaimVerdict, SourceWithMetadata } from '@/types/conversation';

import { ClaimActionsPopover } from '../actions/ClaimActionsPopover';

const navigate = vi.hoisted(() => vi.fn());
const quickCapture = vi.hoisted(() =>
  vi.fn().mockResolvedValue({ ok: true, data: { noteId: 'note-1', noteTitle: 'Week of Sep 14' } }),
);
const writeText = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return { ...actual, useNavigate: () => navigate };
});

vi.mock('@/lib/api', () => ({
  VaultAPI: { quickCapture },
  default: { quickCapture },
}));

vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ conversations: [{ id: 'conv-1', title: 'Canopy cooling' }] }),
}));

const source = (documentId: string, fileName: string, extra: Partial<SourceWithMetadata> = {}) =>
  ({
    documentId,
    chunkId: `${documentId}#c1`,
    fileName,
    filePath: `/vault/${fileName}`,
    mimeType: 'application/pdf',
    category: 'Research Paper',
    content: 'A cited passage.',
    excerpt: 'A cited passage.',
    score: 0.9,
    fileSizeBytes: 100,
    modifiedAt: '2026-08-01T00:00:00.000Z',
    ...extra,
  }) as SourceWithMetadata;

const citationMap = new Map<number, SourceWithMetadata>([
  [1, source('doc-halvorsen', 'Halvorsen 2024.pdf', { pageNumber: 14 })],
  [2, source('doc-transect', 'Transect.md')],
]);

const backed: ClaimVerdict = {
  sentence: 'The pooled estimate was 1.2 °C per 10 points of canopy.',
  citationIds: [1],
  verdict: 'supported',
  evidenceQuote: 'the pooled estimate was a 1.2 °C reduction',
  method: 'judge',
};

function renderPopover(verdict: ClaimVerdict = backed, onClose = vi.fn()) {
  render(
    <MemoryRouter>
      <ClaimActionsPopover
        anchor={new DOMRect(120, 400, 320, 22)}
        verdict={verdict}
        citationMap={citationMap}
        conversationId="conv-1"
        onClose={onClose}
      />
    </MemoryRouter>,
  );
  return onClose;
}

describe('ClaimActionsPopover', () => {
  beforeEach(() => {
    navigate.mockClear();
    quickCapture.mockClear();
    writeText.mockClear();
    Object.assign(navigator, { clipboard: { writeText } });
  });

  it('says what the check made of the sentence before offering anything', () => {
    renderPopover();
    expect(screen.getByText('This sentence is backed by its source')).toBeInTheDocument();
  });

  it('copies the sentence with the file and page it came from', async () => {
    const onClose = renderPopover();
    await userEvent.click(screen.getByText('Copy with citation'));

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText).toHaveBeenCalledWith(
      'The pooled estimate was 1.2 °C per 10 points of canopy.\n\n— Halvorsen 2024.pdf, PDF p. 14',
    );
    expect(onClose).toHaveBeenCalled();
  });

  it('writes the sentence, its evidence and the verdict to the journal', async () => {
    renderPopover();
    await userEvent.click(screen.getByText('Add to journal'));

    await waitFor(() => expect(quickCapture).toHaveBeenCalledTimes(1));
    const written = quickCapture.mock.calls[0]![0] as string;
    expect(written).toContain('## From chat · Canopy cooling');
    expect(written).toContain('> The pooled estimate was 1.2 °C per 10 points of canopy.');
    expect(written).toContain('Evidence: “the pooled estimate was a 1.2 °C reduction”');
  });

  it('asks an ungrounded sentence for its source, through the composer prefill', async () => {
    renderPopover({ ...backed, verdict: 'unsupported', evidenceQuote: null, citationIds: [] });
    await userEvent.click(screen.getByText('Ask why'));

    expect(navigate).toHaveBeenCalledTimes(1);
    const target = navigate.mock.calls[0]![0] as string;
    expect(target.startsWith('/chat?quote=')).toBe(true);
    // Read back exactly as ChatView reads it, through the search params.
    const quote = new URLSearchParams(target.slice('/chat?'.length)).get('quote') ?? '';
    expect(quote).toContain('The pooled estimate was 1.2 °C per 10 points of canopy.');
    expect(quote).toContain('Where in my documents does this come from?');
  });

  it('offers Compare only when the sentence rests on more than one document', async () => {
    renderPopover();
    expect(screen.queryByText('Compare its sources')).not.toBeInTheDocument();

    renderPopover({ ...backed, citationIds: [1, 2] });
    await userEvent.click(screen.getAllByText('Compare its sources')[0]!);
    expect(navigate).toHaveBeenCalledWith('/compare?ids=doc-halvorsen%2Cdoc-transect');
  });

  it('is reachable from the keyboard: the first verb takes focus and arrows move on', async () => {
    renderPopover();
    await waitFor(() =>
      expect(document.activeElement?.textContent).toContain('Copy with citation'),
    );

    await userEvent.keyboard('{ArrowDown}');
    expect(document.activeElement?.textContent).toContain('Add to journal');
    await userEvent.keyboard('{ArrowUp}');
    expect(document.activeElement?.textContent).toContain('Copy with citation');
  });

  it('closes on Escape and on a click outside it', async () => {
    const onClose = renderPopover();
    await userEvent.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);

    await userEvent.click(document.body);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
