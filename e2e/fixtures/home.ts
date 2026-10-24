import type { Page } from '@playwright/test';
import type { ConversationDto, ConversationJournalDto, DocumentMetadataDto, IndexedFolder } from '../../src/lib/bindings';
import { installReferencesFixture } from './references';
import { serveCommands, type Fixture } from './tauri';

const FILES = '/home/test/.lattice/files';
const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString();

export const FIELD_JOURNAL: ConversationJournalDto = {
  id: 'journal-field', name: 'Field journal', description: null, icon: null, accentColor: null,
  spacePrompt: null, toolPreferencesJson: null, isArchived: false, sortOrder: 0, createdAt: daysAgo(30), updatedAt: daysAgo(0),
};

function conversation(id: string, title: string, spaceId: string, updatedDaysAgo: number, preview: string): Fixture<ConversationDto> {
  return {
    id, title, spaceId, isArchived: false, messageCount: 4, totalTokens: 900,
    createdAt: daysAgo(updatedDaysAgo + 1), updatedAt: daysAgo(updatedDaysAgo), lastMessagePreview: preview,
  };
}

function document(id: string, fileName: string, fileType: string, indexedDaysAgo: number): Fixture<DocumentMetadataDto> {
  return {
    id, fileName, filePath: `${FILES}/${id}/${fileName}`, fileType, category: 'Document', language: 'en',
    wordCount: 2400, modifiedAt: daysAgo(indexedDaysAgo), indexedAt: daysAgo(indexedDaysAgo),
  };
}

/** A library with a little of everything: documents, folders, chats, a journal entry and a saved answer. */
export function homeLibrary() {
  return {
    documents: [
      document('doc-cells', 'Cell biology primer.md', 'md', 0),
      document('doc-medieval', 'Medieval England.md', 'md', 2),
      document('doc-songbirds', 'Field recordings of songbirds.pdf', 'pdf', 5),
    ],
    folders: [
      { path: '/home/test/Notes', recursive: true, enabled: true, lastScan: daysAgo(0), documentCount: 2, createdAt: daysAgo(30) },
      { path: '/home/test/Archive', recursive: false, enabled: true, lastScan: daysAgo(5), documentCount: 1, createdAt: daysAgo(30) },
    ] satisfies IndexedFolder[],
    conversations: [
      conversation('chat-bread', 'Bread baking', 'space_general', 1, 'An autolyse rests flour and water before the salt goes in.'),
      conversation('chat-roads', 'Roman roads', 'space_general', 3, 'The Via Appia ran from Rome to Brindisi.'),
      // A journal entry: shown under Today, never under Continue.
      conversation('entry-morning', 'Morning birdsong count', FIELD_JOURNAL.id, 0, 'Eleven robins and a wren by the hedge.'),
    ],
    journals: [FIELD_JOURNAL],
  };
}

export async function installHomeFixture(page: Page) {
  // Home links straight into a reference, so the inbox's commands are part of the journey.
  const references = await installReferencesFixture(page);
  // One backend: the inbox's passages and bookmarks are the ones Home shows.
  const state = Object.assign(references.state, homeLibrary(), {
    /** While set, the indexing stats Home is built from fail with this message. */
    statsError: null as string | null,
  });
  await serveCommands(page, () => ({
    get_indexing_stats: () => {
      if (state.statsError) throw new Error(state.statsError);
      return { indexedDocuments: state.documents.length, totalChunks: state.documents.length * 40 };
    },
    list_all_documents: () => state.documents,
    get_system_stats: () => ({ total_documents: state.documents.length, total_chunks: state.documents.length * 40, total_tags: 0, storage_size_bytes: state.documents.length * 1_048_576 }),
    get_indexed_folders: () => state.folders,
    list_conversations: () => ({ conversations: state.conversations, total: state.conversations.length }),
    list_journals: () => state.journals,
    list_journal_conversations: () => {
      const entries = state.conversations.filter(item => item.spaceId === FIELD_JOURNAL.id);
      return { conversations: entries, total: entries.length };
    },
    get_corpus_shape: () => ({ total: state.documents.length, byType: [{ type: 'md', count: 2 }, { type: 'pdf', count: 1 }], grownLast7Days: state.documents.length }),
  }));
  return { ...references, state };
}
