import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { EntryEditor } from '@/features/journal/components/EntryEditor';

vi.mock('@/components/TiptapEditor', () => ({
  TiptapEditor: ({
    ariaLabel,
    citationNumbers,
    onCitationClick,
  }: {
    ariaLabel?: string;
    citationNumbers?: readonly number[];
    onCitationClick?: (number: number, occurrence: number) => void;
  }) => (
    <>
      <div role="textbox" aria-label={ariaLabel} data-citations={citationNumbers?.join(',')} />
      {citationNumbers?.map((num) => (
        <button key={num} type="button" onClick={() => onCitationClick?.(num, 0)}>
          Cite [{num}]
        </button>
      ))}
    </>
  ),
}));
vi.mock('@/features/journal/components/EntryFromConversation', () => ({ EntryFromConversation: () => <div>Conversation panel</div> }));
vi.mock('@/features/journal/components/EntryHighlightsStrip', () => ({
  EntryHighlightsStrip: () => <div>Highlights panel</div>,
  HIGHLIGHT_CHAR_LIMIT: 500,
}));
vi.mock('@/features/journal/components/EntryActionRail', () => ({ EntryActionRail: () => <div>Actions</div> }));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({ FilePreviewModal: () => null }));

function renderEmptyEditor() {
  return render(
    <MemoryRouter>
      <TooltipProvider>
        <EntryEditor
          activeNote={{
            id: 'page-1',
            revision: 0,
            title: 'Untitled page',
            journalId: 'journal-1',
            content: '',
            linkedDocumentIds: [],
            linkedConversationIds: [],
            highlights: [],
            stickyNotes: [],
            conversationSnapshots: [],
            sources: [],
            createdAt: '2026-10-01T00:00:00Z',
            updatedAt: '2026-10-01T00:00:00Z',
          }}
          isLoadingNote={false}
          loadError={null}
          saveError={null}
          hasPendingChanges={false}
          isSaving={false}
          onSaveNow={vi.fn()}
          onUpdateNoteContent={vi.fn()}
          onAddHighlight={vi.fn()}
          onRemoveHighlight={vi.fn()}
          pinnedHighlightIds={new Set()}
          onTogglePinnedHighlight={vi.fn()}
          journalName="Journal"
          pageTitle={null}
          isDefaultPage
          onRenamePage={vi.fn()}
          selectedEntry={null}
          selectedEntryMessages={[]}
          selectedEntryLoading={false}
          hasConversations={false}
          hasRequestedEntry={false}
          pinnedEntryCount={0}
          deckCount={0}
          crossEntrySources={[]}
          crossEntryLoading={false}
          scannedConversationCount={0}
          onJumpToEntry={vi.fn()}
          onSynthesize={vi.fn(async () => true)}
          onNotify={vi.fn()}
        />
      </TooltipProvider>
    </MemoryRouter>,
  );
}

describe('EntryEditor context rail defaults and shortcut', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('defaults the context rail to collapsed when empty and toggles with button and Mod+.', () => {
    renderEmptyEditor();

    expect(screen.queryByRole('complementary', { name: 'Beside this page' })).not.toBeInTheDocument();
    const showButton = screen.getByRole('button', { name: 'Show conversation and highlights' });
    expect(showButton).toBeInTheDocument();

    fireEvent.click(showButton);
    expect(screen.getByRole('complementary', { name: 'Beside this page' })).toBeInTheDocument();
    expect(localStorage.getItem('journal.contextRail.open')).toBe('1');

    fireEvent.keyDown(window, { key: '.', ctrlKey: true });
    expect(screen.queryByRole('complementary', { name: 'Beside this page' })).not.toBeInTheDocument();
    expect(localStorage.getItem('journal.contextRail.open')).toBe('0');
  });

  it('opens the context rail and switches to Sources when an inline citation is clicked, supporting citationNumber', () => {
    localStorage.setItem('journal.contextRail.open', '0');
    render(
      <MemoryRouter>
        <TooltipProvider>
          <EntryEditor
            activeNote={{
              id: 'page-2',
              revision: 0,
              title: 'Garden',
              journalId: 'journal-1',
              content: 'Followed a week later [1].',
              linkedDocumentIds: [],
              linkedConversationIds: [],
              highlights: [],
              stickyNotes: [],
              conversationSnapshots: [],
              sources: [
                {
                  documentId: 'doc-1',
                  chunkId: 'chunk-1',
                  fileName: 'Botany.md',
                  filePath: '/Botany.md',
                  mimeType: 'text/markdown',
                  category: 'Markdown',
                  content: 'Seedling notes.',
                  score: 0.9,
                  fileSizeBytes: 10,
                  modifiedAt: '2026-10-01T00:00:00Z',
                  citationNumber: 1,
                  path: null,
                  position: null,
                } as never,
              ],
              createdAt: '2026-10-01T00:00:00Z',
              updatedAt: '2026-10-01T00:00:00Z',
            }}
            isLoadingNote={false}
            loadError={null}
            saveError={null}
            hasPendingChanges={false}
            isSaving={false}
            onSaveNow={vi.fn()}
            onUpdateNoteContent={vi.fn()}
            onAddHighlight={vi.fn()}
            onRemoveHighlight={vi.fn()}
            pinnedHighlightIds={new Set()}
            onTogglePinnedHighlight={vi.fn()}
            journalName="Journal"
            pageTitle="Garden"
            isDefaultPage={false}
            onRenamePage={vi.fn()}
            selectedEntry={null}
            selectedEntryMessages={[]}
            selectedEntryLoading={false}
            hasConversations={false}
            hasRequestedEntry={false}
            pinnedEntryCount={0}
            deckCount={0}
            crossEntrySources={[]}
            crossEntryLoading={false}
            scannedConversationCount={0}
            onJumpToEntry={vi.fn()}
            onSynthesize={vi.fn(async () => true)}
            onNotify={vi.fn()}
          />
        </TooltipProvider>
      </MemoryRouter>,
    );

    expect(screen.getByRole('textbox', { name: 'Journal entry' })).toHaveAttribute('data-citations', '1');
    expect(screen.queryByRole('complementary', { name: 'Beside this page' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Cite [1]' }));

    expect(screen.getByRole('complementary', { name: 'Beside this page' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Sources/ })).toHaveAttribute('aria-selected', 'true');
  });
});
