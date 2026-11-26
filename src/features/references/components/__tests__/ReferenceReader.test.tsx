import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, expect, it, vi } from 'vitest';

import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { ReferenceReader } from '@/features/references/components/ReferenceReader';
import type { ReplyPieceDto } from '@/lib/bindings';
import { reply } from '@/tests/replyFixtures';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { SourceWithMetadata } from '@/types/conversation';
import { withoutMarks } from '@/utils/replyPieces';

vi.mock('@/features/reading/components/SourceCitations', () => ({
  SourceCitations: () => <div>Saved source cards</div>,
}));

beforeEach(() => citationDisplayStore.getState().setVisible(true));

it('opens the original citation ID and shares clean reading across the reference body and source cards', async () => {
  const source: SourceWithMetadata = {
    documentId: 'document', chunkId: 'chunk', fileName: 'Source', filePath: '/docs/source.md',
    mimeType: 'text/markdown', category: 'Document', content: 'Saved source evidence.',
    score: 1, fileSizeBytes: 22, modifiedAt: '2026-10-08T12:00:00Z', citationId: 7,
  };
  const bookmark = {
    id: 'bookmark', conversationId: 'conversation', conversationTitle: 'Saved conversation', messageId: 'message',
    messageRole: 'assistant', messagePreview: 'A claim [7].', title: null, note: null, createdAt: '2026-10-08T12:00:00Z',
  } as ConversationMessageBookmarkDto;
  const onViewSource = vi.fn();
  const { container } = render(<MemoryRouter><ReferenceReader
    bookmark={bookmark} space={null} capture={null} resolutionFailed={false} captureDestination={null}
    payload={{ content: 'A claim [s7]. Step [1] is ordinary text.', reply: reply('A claim ', 7, '. Step [1] is ordinary text.'), sources: [source], sourceReferences: [] }}
    onViewSource={onViewSource} onSaveAnnotations={vi.fn(async () => true)}
    onOpenInChat={vi.fn()} onOpenConversation={vi.fn()} onOpenCapturedNote={vi.fn()}
    onCapture={vi.fn(async () => {})} onCopy={vi.fn(async () => true)} onDelete={vi.fn()}
  /></MemoryRouter>);

  fireEvent.click(await screen.findByRole('button', { name: 'Citation 7' }));
  expect(onViewSource).toHaveBeenCalledWith(source, 0);
  expect(screen.getByText('Saved source cards')).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Hide citations' }));
  await waitFor(() => expect(container.querySelector('.citation-hidden')).not.toBeNull());
  expect(screen.queryByText('Saved source cards')).not.toBeInTheDocument();
  expect(container).toHaveTextContent('Step [1] is ordinary text.');
  expect(citationDisplayStore.getState().visible).toBe(false);

  fireEvent.click(screen.getByRole('button', { name: 'Show citations' }));
  expect(await screen.findByRole('button', { name: 'Citation 7' })).toBeVisible();
  expect(screen.getByText('Saved source cards')).toBeVisible();
});

it('shows CitationCard hover support, Quoted marks, and forwards anchored occurrence on chip click', async () => {
  const snapshotText = '# Handbook\n\n## Hold\n\nKeep leaves dry before packing.';
  const source: SourceWithMetadata = {
    documentId: 'web:https://example.org/handbook',
    chunkId: 'chunk-7',
    fileName: 'Handbook',
    filePath: 'https://example.org/handbook',
    mimeType: 'text/html',
    category: 'Web article',
    content: 'Keep leaves dry before packing.',
    webSnapshot: { url: 'https://example.org/handbook', title: 'Handbook', text: snapshotText, fetchedAt: null, truncated: false },
    score: 1,
    fileSizeBytes: 0,
    modifiedAt: '2026-10-08T12:00:00Z',
    citationId: 7,
  };
  const pieces: ReplyPieceDto[] = [
    { type: 'quoted', text: 'Keep leaves dry before packing.' },
    { type: 'text', text: ' ' },
    {
      type: 'reference',
      reference: {
        written: '[s7]',
        kind: 'source',
        number: 7,
        support: { relation: 'quotes', anchors: [{ exact: 'Keep leaves dry before packing.' }] },
      },
    },
  ];
  const bookmark = {
    id: 'bookmark-2',
    conversationId: 'conversation',
    conversationTitle: 'Tea storage',
    messageId: 'message-2',
    messageRole: 'assistant',
    messagePreview: 'Keep leaves dry.',
    title: null,
    note: null,
    createdAt: '2026-10-08T12:00:00Z',
  } as ConversationMessageBookmarkDto;
  const onViewSource = vi.fn();

  const { container } = render(
    <MemoryRouter>
      <ReferenceReader
        bookmark={bookmark}
        space={null}
        capture={null}
        resolutionFailed={false}
        captureDestination={null}
        payload={{ content: 'Keep leaves dry before packing. [s7]', reply: pieces, sources: [source], sourceReferences: [] }}
        onViewSource={onViewSource}
        onSaveAnnotations={vi.fn(async () => true)}
        onOpenInChat={vi.fn()}
        onOpenConversation={vi.fn()}
        onOpenCapturedNote={vi.fn()}
        onCapture={vi.fn(async () => {})}
        onCopy={vi.fn(async () => true)}
        onDelete={vi.fn()}
      />
    </MemoryRouter>,
  );

  expect(Array.from(container.querySelectorAll('.cite-quote')).map((el) => el.textContent)).toEqual(['“', '”']);
  expect(withoutMarks(container.textContent ?? '')).toContain('“Keep leaves dry before packing.”');

  const chip = await screen.findByRole('button', { name: 'Citation 7' });
  fireEvent.mouseOver(chip);
  const tooltip = await screen.findByRole('tooltip');
  expect(tooltip).toHaveTextContent('Quoted from');
  expect(tooltip).toHaveTextContent('“Keep leaves dry before packing.”');
  expect(tooltip).toHaveTextContent('Handbook › Hold');

  fireEvent.click(chip);
  expect(onViewSource).toHaveBeenCalledWith(
    expect.objectContaining({
      citationId: 7,
      anchoredBy: [{ occurrence: 0, relation: 'quotes', anchors: [{ exact: 'Keep leaves dry before packing.' }] }],
    }),
    0,
  );
});
