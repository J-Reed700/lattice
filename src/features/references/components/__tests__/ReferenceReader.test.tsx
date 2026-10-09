import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, expect, it, vi } from 'vitest';

import { ReferenceReader } from '@/features/references/components/ReferenceReader';
import { citationDisplayStore } from '@/stores/citationDisplayStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type { SourceWithMetadata } from '@/types/conversation';

vi.mock('@/features/chat/components/SourceCitations', () => ({
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
    payload={{ content: 'A claim [7]. Step [1] is ordinary text.', sources: [source], sourceReferences: [] }}
    onViewSource={onViewSource} onSaveAnnotations={vi.fn(async () => true)}
    onOpenInChat={vi.fn()} onOpenConversation={vi.fn()} onOpenCapturedNote={vi.fn()}
    onCapture={vi.fn(async () => {})} onCopy={vi.fn(async () => true)} onDelete={vi.fn()}
  /></MemoryRouter>);

  fireEvent.click(await screen.findByRole('button', { name: 'Citation 7' }));
  expect(onViewSource).toHaveBeenCalledWith(source);
  expect(screen.getByText('Saved source cards')).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Hide citations' }));
  await waitFor(() => expect(container.querySelector('.citation-hidden')).toHaveTextContent('[7]'));
  expect(screen.queryByText('Saved source cards')).not.toBeInTheDocument();
  expect(container).toHaveTextContent('Step [1] is ordinary text.');
  expect(citationDisplayStore.getState().visible).toBe(false);

  fireEvent.click(screen.getByRole('button', { name: 'Show citations' }));
  expect(await screen.findByRole('button', { name: 'Citation 7' })).toBeVisible();
  expect(screen.getByText('Saved source cards')).toBeVisible();
});
