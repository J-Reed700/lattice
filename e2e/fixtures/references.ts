import type { Page } from '@playwright/test';
import type { ConversationMessageBookmarkDto, GetConversationMessagesRequestDto, ListMessageBookmarksQueryDto, PassageReferenceDto, UpdatePassageReferenceRequestDto } from '../../src/lib/bindings';
import { installShell, type ShellBackend } from './shell';
import { serveCommands } from './tauri';

const FILES = '/home/test/.lattice/files';
const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString();

/** Saved passages from two documents, on different subjects and days. */
export function savedPassages(): PassageReferenceDto[] {
  return [
    {
      id: 'ref-mitochondria', documentId: 'doc-cells', chunkId: 'cells-chunk-4',
      filePath: `${FILES}/doc-cells/Cell biology primer.md`, fileName: 'Cell biology primer.md', locator: 'Chapter 3',
      text: 'Mitochondria generate most of the cell’s ATP through oxidative phosphorylation.',
      title: null, note: null, createdAt: daysAgo(0),
    },
    {
      id: 'ref-magna-carta', documentId: 'doc-medieval', chunkId: 'medieval-chunk-9',
      filePath: `${FILES}/doc-medieval/Medieval England.md`, fileName: 'Medieval England.md', locator: null,
      text: 'Magna Carta (1215) bound the king to the law of the land.',
      title: 'Rule of law', note: 'Compare with the 1689 Bill of Rights.', createdAt: daysAgo(40),
    },
  ];
}

/** A bookmarked chat answer, resolved against its conversation's messages. */
export const BREAD_BOOKMARK: ConversationMessageBookmarkDto = {
  id: 'bookmark-autolyse', conversationId: 'chat-bread', conversationTitle: 'Bread baking', spaceId: 'space_general',
  messageId: 'msg-autolyse', messageRole: 'assistant',
  messagePreview: 'An autolyse rests flour and water before the salt goes in.',
  title: null, note: null, createdAt: daysAgo(1),
};
const BREAD_ANSWER = 'An autolyse rests flour and water for 20 to 60 minutes before the salt goes in, so the gluten starts forming on its own.';

export async function installReferencesFixture(page: Page) {
  const shell = await installShell(page);
  const state = {
    passages: savedPassages(),
    bookmarks: [BREAD_BOOKMARK],
    /** While set, listing saved passages fails with this message. */
    listError: null as string | null,
    /** The next annotation save fails with this message, once. */
    failNextUpdate: null as string | null,
    /** Holds bookmark replies until released, so passages can arrive first. */
    holdBookmarks: null as Promise<void> | null,
    /** Holds saved-passage replies until released, so bookmarks can arrive first. */
    holdPassages: null as Promise<void> | null,
  };
  const passage = (id: string) => {
    const found = state.passages.find(item => item.id === id);
    if (!found) throw new Error(`Reference ${id} no longer exists.`);
    return found;
  };
  await serveCommands(page, () => ({
    list_passage_references: async () => {
      if (state.holdPassages) await state.holdPassages;
      if (state.listError) throw new Error(state.listError);
      return state.passages;
    },
    update_passage_reference: (args) => {
      const request = args.request as UpdatePassageReferenceRequestDto;
      if (state.failNextUpdate) {
        const message = state.failNextUpdate;
        state.failNextUpdate = null;
        throw new Error(message);
      }
      const target = passage(request.id);
      target.title = request.title;
      target.note = request.note;
      return target;
    },
    delete_passage_reference: (args) => {
      passage(String(args.id));
      state.passages = state.passages.filter(item => item.id !== args.id);
      return null;
    },
    // Bookmark search runs in the backend.
    list_message_bookmarks: async (args) => {
      if (state.holdBookmarks) await state.holdBookmarks;
      const query = (args.query as ListMessageBookmarksQueryDto).query?.toLowerCase() ?? '';
      const bookmarks = state.bookmarks.filter(item => !query || `${item.conversationTitle} ${item.messagePreview}`.toLowerCase().includes(query));
      return { bookmarks, total: bookmarks.length };
    },
    get_conversation_messages: (args) => {
      if ((args.request as GetConversationMessagesRequestDto).conversationId !== BREAD_BOOKMARK.conversationId) throw new Error('Conversation not found');
      const messages = [
        { id: 'msg-question', conversationId: 'chat-bread', role: 'user', content: 'What is an autolyse?', tokens: 6, status: 'completed', createdAt: daysAgo(1), metadata: null },
        { id: BREAD_BOOKMARK.messageId, conversationId: 'chat-bread', role: 'assistant', content: BREAD_ANSWER, tokens: 30, status: 'completed', createdAt: daysAgo(1), metadata: null },
      ];
      return { messages, total: messages.length };
    },
    list_journals: () => [],
    list_workspace_notes: () => ({ notes: [] }),
    read_file_content: (args) => {
      const found = state.passages.find(item => item.filePath === args.path) ?? savedPassages().find(item => item.filePath === args.path);
      if (!found) throw new Error(`No such file: ${String(args.path)}`);
      return `# ${found.fileName.replace(/\.md$/, '')}\n\nAn opening paragraph.\n\n${found.text}\n\nA closing paragraph.\n`;
    },
  }));
  return { ...shell, state } satisfies ShellBackend & { state: typeof state };
}
