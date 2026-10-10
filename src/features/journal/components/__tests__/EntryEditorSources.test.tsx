import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { EntryEditor } from '@/features/journal/components/EntryEditor';
import { useChatReaderStore } from '@/features/reading/stores/chatReaderStore';


vi.mock('@/components/TiptapEditor', () => ({
  TiptapEditor: ({ onCitationClick }: { onCitationClick?: (number: number, occurrence: number) => void }) => (
    <button type="button" onClick={() => onCitationClick?.(2, 1)}>Inline citation [2]</button>
  ),
}));
vi.mock('@/features/journal/components/EntryHeader', () => ({ EntryHeader: () => <div>Page header</div> }));
vi.mock('@/features/journal/components/EntryFromConversation', () => ({ EntryFromConversation: () => <div>Conversation panel</div> }));
vi.mock('@/features/journal/components/EntryHighlightsStrip', () => ({
  EntryHighlightsStrip: () => <div>Highlights panel</div>,
  HIGHLIGHT_CHAR_LIMIT: 500,
}));
vi.mock('@/features/journal/components/EntryActionRail', () => ({ EntryActionRail: () => <div>Actions</div> }));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: (props: { source: { documentId: string; chunkId: string; pageNumber?: number; content: string }; citationContent?: string; occurrence?: number | null }) => (
    <div data-testid="source-reader" data-document={props.source?.documentId} data-chunk={props.source?.chunkId} data-page={props.source?.pageNumber}
      data-occurrence={props.occurrence ?? ''} data-content={props.citationContent} data-passage={props.source?.content} />
  ),
}));

const source = {
  documentId: 'doc-1', chunkId: 'chunk-main', fileName: 'Study.pdf', filePath: '/Study.pdf',
  mimeType: 'application/pdf', category: 'PDF Document', content: 'Main excerpt', score: 0.9,
  fileSizeBytes: 10, modifiedAt: '2026-09-01T00:00:00Z', citationId: 2,
  path: null, position: null,
  chunkExcerpts: [
    { chunkId: 'chunk-main', excerpt: 'Main excerpt', score: 0.9, pageNumber: 2 },
    { chunkId: 'chunk-secondary', excerpt: 'Secondary exact passage', score: 0.8, pageNumber: 7 },
  ],
};

function renderEditor(sources = [source]) {
  return render(
    <MemoryRouter>
      <TooltipProvider>
        <EntryEditor
          activeNote={{
            id: 'page-1', revision: 0, title: 'Research', journalId: 'journal-1',
            content: 'The result is clear [2]. A second claim [2].',
            linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [],
            conversationSnapshots: [], sources, createdAt: '', updatedAt: '',
          }}
          isLoadingNote={false} loadError={null} saveError={null} hasPendingChanges={false} isSaving={false}
          onSaveNow={vi.fn()} onUpdateNoteContent={vi.fn()} onAddHighlight={vi.fn()} onRemoveHighlight={vi.fn()}
          pinnedHighlightIds={new Set()} onTogglePinnedHighlight={vi.fn()} journalName="Journal" pageTitle="Research"
          isDefaultPage={false} onRenamePage={vi.fn()} selectedEntry={null} selectedEntryMessages={[]}
          selectedEntryLoading={false} pinnedEntryCount={0} deckCount={0} crossEntrySources={[]}
          crossEntryLoading={false} scannedConversationCount={0} onJumpToEntry={vi.fn()}
          onSynthesize={vi.fn(async () => true)} onNotify={vi.fn()}
        />
      </TooltipProvider>
    </MemoryRouter>,
  );
}

describe('EntryEditor source citations', () => {
  beforeEach(() => {
    useChatReaderStore.setState({ session: null });
  });

  it('opens the nested cited passage from Sources and the clicked inline citation with its occurrence', () => {
    renderEditor();
    fireEvent.click(screen.getByRole('tab', { name: /Sources/ }));
    fireEvent.click(screen.getByRole('button', { name: /1 source/ }));
    fireEvent.click(screen.getAllByRole('button', { name: /View passage/ })[1]!);

    let reader = screen.getByTestId('source-reader');
    expect(reader).toHaveAttribute('data-chunk', 'chunk-secondary');
    expect(reader).toHaveAttribute('data-page', '7');
    expect(reader).toHaveAttribute('data-content', 'The result is clear [2]. A second claim [2].');

    fireEvent.click(screen.getByRole('button', { name: 'Inline citation [2]' }));
    reader = screen.getByTestId('source-reader');
    expect(reader).toHaveAttribute('data-chunk', 'chunk-main');
    expect(reader).toHaveAttribute('data-occurrence', '1');
  });

  it('uses document identity when two sources reuse a web chunk ID', () => {
    const secondSource = {
      ...source,
      documentId: 'doc-2',
      fileName: 'Second study.pdf',
      filePath: 'https://example.org/second-study',
      citationId: 3,
    };
    renderEditor([source, secondSource]);
    fireEvent.click(screen.getByRole('tab', { name: /Sources/ }));
    fireEvent.click(screen.getByRole('button', { name: /2 sources/ }));
    fireEvent.click(screen.getAllByRole('button', { name: /View passage/ })[2]!);

    expect(screen.getByTestId('source-reader')).toHaveAttribute('data-document', 'doc-2');
  });

  it('opens the exact saved snapshot when the same document reuses a chunk ID', () => {
    const sharedOpening = 'This saved passage starts with exactly the same wording. '.repeat(2);
    const firstSnapshot = {
      ...source,
      chunkExcerpts: [
        { chunkId: 'chunk-main', excerpt: 'Original main excerpt', score: 0.9, pageNumber: 4 },
        { chunkId: 'chunk-secondary', excerpt: `${sharedOpening}original ending`, score: 0.8, pageNumber: 10 },
      ],
    };
    const laterSnapshot = {
      ...source,
      citationId: 4,
      content: 'Later snapshot main excerpt',
      chunkExcerpts: [
        { chunkId: 'chunk-main', excerpt: 'Later snapshot main excerpt', score: 0.9, pageNumber: 4 },
        { chunkId: 'chunk-secondary', excerpt: `${sharedOpening}later ending`, score: 0.8, pageNumber: 11 },
      ],
    };
    renderEditor([firstSnapshot, laterSnapshot]);
    fireEvent.click(screen.getByRole('tab', { name: /Sources/ }));
    fireEvent.click(screen.getByRole('button', { name: /1 source/ }));
    fireEvent.click(screen.getAllByRole('button', { name: /View passage/ })[3]!);

    const reader = screen.getByTestId('source-reader');
    expect(reader).toHaveAttribute('data-page', '11');
    expect(reader).toHaveAttribute('data-passage', `${sharedOpening}later ending`);
  });
});
