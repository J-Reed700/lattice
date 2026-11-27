import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { usePinnedNoteHighlights, useSidebarCollapsed } from '../useJournalViewPrefs';

function makeNote(id: string, journalId: string, highlightIds: string[]): WorkspaceNote {
  return {
    id,
    revision: 0,
    title: id,
    journalId,
    content: '',
    linkedDocumentIds: [],
    linkedConversationIds: [],
    highlights: highlightIds.map((hid) => ({
      id: hid,
      text: `Highlight ${hid}`,
      createdAt: '',
    })),
    stickyNotes: [],
    conversationSnapshots: [],
    sources: [],
    citations: [],
    createdAt: '',
    updatedAt: '',
  };
}

describe('useJournalViewPrefs', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('persists sidebar collapsed state across mounts', () => {
    const first = renderHook(() => useSidebarCollapsed());
    expect(first.result.current[0]).toBe(false);
    act(() => {
      first.result.current[1](true);
    });
    expect(first.result.current[0]).toBe(true);
    first.unmount();

    const second = renderHook(() => useSidebarCollapsed());
    expect(second.result.current[0]).toBe(true);
  });

  it('preserves pinned highlights per page when switching pages in the same journal', () => {
    const pageA = makeNote('page-a', 'journal-1', ['h-a1', 'h-a2']);
    const pageB = makeNote('page-b', 'journal-1', ['h-b1']);

    const { result, rerender } = renderHook(
      ({ spaceId, note }: { spaceId: string | null; note: WorkspaceNote | null }) =>
        usePinnedNoteHighlights(spaceId, note),
      { initialProps: { spaceId: 'journal-1', note: pageA } },
    );

    act(() => {
      result.current.handleTogglePinnedHighlight('h-a1');
    });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-a1']);

    // Switch to page B and pin h-b1
    rerender({ spaceId: 'journal-1', note: pageB });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual([]);
    act(() => {
      result.current.handleTogglePinnedHighlight('h-b1');
    });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-b1']);

    // Switch back to page A: h-a1 must still be pinned
    rerender({ spaceId: 'journal-1', note: pageA });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-a1']);

    // Switch back to page B: h-b1 must still be pinned
    rerender({ spaceId: 'journal-1', note: pageB });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-b1']);
  });

  it('prunes deleted highlights only on the active page without touching other pages', () => {
    const pageA = makeNote('page-a', 'journal-1', ['h-a1', 'h-a2']);
    const pageB = makeNote('page-b', 'journal-1', ['h-b1']);

    const { result, rerender } = renderHook(
      ({ spaceId, note }: { spaceId: string | null; note: WorkspaceNote | null }) =>
        usePinnedNoteHighlights(spaceId, note),
      { initialProps: { spaceId: 'journal-1', note: pageA } },
    );

    act(() => {
      result.current.handleTogglePinnedHighlight('h-a1');
      result.current.handleTogglePinnedHighlight('h-a2');
    });

    rerender({ spaceId: 'journal-1', note: pageB });
    act(() => {
      result.current.handleTogglePinnedHighlight('h-b1');
    });

    // Remove h-a1 from page A
    const updatedPageA = makeNote('page-a', 'journal-1', ['h-a2']);
    rerender({ spaceId: 'journal-1', note: updatedPageA });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-a2']);

    // Page B's pin must remain untouched
    rerender({ spaceId: 'journal-1', note: pageB });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-b1']);
  });

  it('never writes the previous journal highlights into a newly selected journal on space switch', () => {
    const space1Note = makeNote('note-1', 'space-1', ['h-1']);
    const space2Note = makeNote('note-2', 'space-2', ['h-2']);
    localStorage.setItem(
      'journal.pinnedNoteHighlights.space-2',
      JSON.stringify({ 'note-2': ['h-2'] }),
    );

    const { result, rerender } = renderHook(
      ({ spaceId, note }: { spaceId: string | null; note: WorkspaceNote | null }) =>
        usePinnedNoteHighlights(spaceId, note),
      { initialProps: { spaceId: 'space-1', note: space1Note } },
    );

    act(() => {
      result.current.handleTogglePinnedHighlight('h-1');
    });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-1']);

    // Simulate the intermediate render where spaceId updates before activeNote finishes loading
    rerender({ spaceId: 'space-2', note: space1Note });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual([]);
    expect(JSON.parse(localStorage.getItem('journal.pinnedNoteHighlights.space-2') ?? '{}')).toEqual({
      'note-2': ['h-2'],
    });

    // Now space-2's activeNote finishes loading
    rerender({ spaceId: 'space-2', note: space2Note });
    expect([...result.current.pinnedNoteHighlightIds]).toEqual(['h-2']);
    expect(JSON.parse(localStorage.getItem('journal.pinnedNoteHighlights.space-1') ?? '{}')).toEqual({
      'note-1': ['h-1'],
    });
  });
});
