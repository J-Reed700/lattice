import { randomUUID } from 'node:crypto';
import type { Page } from '@playwright/test';
import type { ConversationJournalDto, CreateConversationJournalRequestDto, CreateWorkspaceNoteRequestDto, ListWorkspaceNotesRequestDto, WorkspaceNoteDto } from '../../src/lib/bindings';
import { installShell, type ShellBackend } from './shell';
import { serveCommands } from './tauri';

const now = () => new Date().toISOString();

/** A journal that already holds one page, for journeys that start mid-use. */
export function gardenJournal(): { journal: ConversationJournalDto; page: WorkspaceNoteDto } {
  const stamp = '2026-10-18T07:15:00Z';
  const journal: ConversationJournalDto = {
    id: 'journal-garden', name: 'Garden log', description: null, icon: null, accentColor: null,
    spacePrompt: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: stamp, updatedAt: stamp,
  };
  const page: WorkspaceNoteDto = {
    id: 'page-tomatoes', title: 'Tomato trial', journalId: journal.id, revision: 3,
    content: 'San Marzano seedlings went out on 2 May; the cherry tomatoes followed a week later.',
    linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [], conversationSnapshots: [], sources: [],
    createdAt: stamp, updatedAt: stamp,
  };
  return { journal, page };
}

export async function installJournalsFixture(page: Page) {
  const shell = await installShell(page);
  const state = {
    journals: [] as ConversationJournalDto[],
    pages: [] as WorkspaceNoteDto[],
    /** While set, listing journals fails with this message. */
    listError: null as string | null,
    /** The next page save fails with this message, once. */
    failNextSave: null as string | null,
  };
  await serveCommands(page, () => ({
    list_journals: () => {
      if (state.listError) throw new Error(state.listError);
      return state.journals;
    },
    create_journal: (args) => {
      const request = args.request as CreateConversationJournalRequestDto;
      const journal: ConversationJournalDto = {
        ...request, id: `journal-${randomUUID()}`, isArchived: false, sortOrder: state.journals.length, createdAt: now(), updatedAt: now(),
      };
      state.journals.push(journal);
      return journal;
    },
    list_workspace_notes: (args) => {
      const journalId = (args.request as ListWorkspaceNotesRequestDto | null)?.journalId;
      return { notes: state.pages.filter(item => !journalId || item.journalId === journalId) };
    },
    create_workspace_note: (args) => {
      const request = args.request as CreateWorkspaceNoteRequestDto;
      const created: WorkspaceNoteDto = {
        id: `page-${randomUUID()}`, title: request.title ?? 'Untitled page', journalId: request.journalId, revision: 0, content: '',
        linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [], conversationSnapshots: [], sources: [],
        createdAt: now(), updatedAt: now(),
      };
      state.pages.push(created);
      return created;
    },
    // Writes are compare-and-swap on the revision, as in the backend.
    update_workspace_note: (args) => {
      const note = args.note as WorkspaceNoteDto;
      if (state.failNextSave) {
        const message = state.failNextSave;
        state.failNextSave = null;
        throw new Error(message);
      }
      const index = state.pages.findIndex(item => item.id === note.id);
      if (index < 0) throw new Error('That page is no longer here.');
      if (state.pages[index].revision !== note.revision) throw new Error('This page changed somewhere else.');
      const saved = { ...note, revision: note.revision + 1, updatedAt: now() };
      state.pages[index] = saved;
      return saved;
    },
    list_journal_conversations: () => ({ conversations: [], total: 0 }),
    list_journal_entry_pins: () => [],
    list_passage_references: () => [],
  }));
  return { ...shell, state } satisfies ShellBackend & { state: typeof state };
}
