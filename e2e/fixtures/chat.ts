import type { Page } from '@playwright/test';
import type { ConversationDto, MessageDto } from '../../src/lib/bindings';
import { makeAppSettings } from '../../src/tests/fixtures/appSettings';
import { gate, installShell, type ShellBackend } from './shell';
import { serveCommands, type Fixture, type MockArgs } from './tauri';

const STAMP = '2026-10-22T14:05:00Z';
const FILES = '/home/test/.lattice/files';

export const QUESTION = 'Why do some volcanoes erupt explosively while others ooze lava?';

/** Two passages from a geology text, the evidence the answer cites. */
export const PASSAGES = [
  {
    citationId: 1, documentId: 'doc-volcanoes', chunkId: 'volcanoes-chunk-2',
    text: 'Basaltic magma is low in silica, so it is runny and lets dissolved gas escape gently.',
  },
  {
    citationId: 2, documentId: 'doc-volcanoes', chunkId: 'volcanoes-chunk-5',
    text: 'Rhyolitic magma is rich in silica and so viscous that gas stays trapped until it bursts out.',
  },
];
const VOLCANO_FILE = `${FILES}/doc-volcanoes/Volcanoes and magma.md`;

/** The answer as it streams in, then as it is saved with its sources. */
export const ANSWER_CHUNKS = ['It comes down to silica. ', 'Runny basaltic magma lets gas escape gently [1]. ', 'Sticky, silica-rich magma traps gas until it bursts out [2].'];
const ANSWER = ANSWER_CHUNKS.join('');

const sources = PASSAGES.map(passage => ({
  documentId: passage.documentId, chunkId: passage.chunkId, citationId: passage.citationId,
  fileName: 'Volcanoes and magma.md', filePath: VOLCANO_FILE, mimeType: 'text/markdown', category: 'Document',
  content: passage.text, score: 0.8, fileSizeBytes: 4096, modifiedAt: STAMP,
}));

export const CONVERSATION: Fixture<ConversationDto> = {
  id: 'chat-volcanoes', title: 'Volcano types', spaceId: 'space_general', isArchived: false,
  createdAt: STAMP, updatedAt: STAMP, messageCount: 0, totalTokens: 0, modelChoice: { kind: 'default' },
};

export async function installChatFixture(page: Page) {
  const shell = await installShell(page);
  // A configured model, so the composer can send.
  const settings = makeAppSettings();
  settings.llm.provider = 'auto';
  settings.llm.llamaCpp.model = 'qwen-test.gguf';
  const state = {
    messages: [] as Fixture<MessageDto>[],
    /** Released by the test to let the running turn finish. */
    finish: gate(),
    /** Released when the reader presses Stop. */
    stopped: gate(),
  };
  await serveCommands(page, () => ({
    get_settings: () => settings,
    list_conversations_explorer: () => ({ conversations: [CONVERSATION], total: 1 }),
    list_conversations: () => ({ conversations: [CONVERSATION], total: 1 }),
    get_conversation: () => ({ conversation: CONVERSATION }),
    get_conversation_messages: () => ({ messages: state.messages, total: state.messages.length }),
    list_message_bookmarks: () => ({ bookmarks: [], total: 0 }),
    list_journals: () => [],
    list_conversation_tangents: () => [],
    list_conversation_linked_documents: () => [],
    list_conversation_web_sources: () => [],
    list_passage_references: () => [],
    get_batch_history: () => ({ jobs: [] }),
    list_loaded_models: () => [],
    generate_chat_starters: () => ({ fingerprint: '', generatedAt: STAMP, starters: [], documentCount: 1 }),
    get_indexing_stats: () => ({ indexedDocuments: 1, totalChunks: 6 }),
    list_all_documents: () => [{
      id: 'doc-volcanoes', fileName: 'Volcanoes and magma.md', filePath: VOLCANO_FILE, fileType: 'md',
      category: 'Document', language: 'en', wordCount: 1800, modifiedAt: STAMP, indexedAt: STAMP,
    }],
    read_file_content: (args) => {
      if (args.path !== VOLCANO_FILE) throw new Error(`No such file: ${String(args.path)}`);
      return ['# Volcanoes and magma', '', 'Magma differs mostly in how much silica it holds.', '', ...PASSAGES.map(passage => passage.text), ''].join('\n\n');
    },
    // One command both runs a turn and, with `cancelOnly`, stops it.
    chat_with_conversation: async (args: MockArgs) => {
      if (args.cancelOnly) {
        state.stopped.release();
        return { conversationId: CONVERSATION.id, message: '', messages: [], contextUsed: 0, sources: [], timingMetrics: null };
      }
      const outcome = await Promise.race([state.finish.released.then(() => 'finished'), state.stopped.released.then(() => 'stopped')]);
      if (outcome === 'stopped') {
        state.stopped = gate();
        // Stopped while searching, before the backend saved the question. Tauri
        // rejects with the serialized ApiError.
        throw new Error(JSON.stringify({ code: 'INVALID_STATE', message: 'Generation was cancelled.' }));
      }
      state.messages = [
        { id: 'msg-question', conversationId: CONVERSATION.id, role: 'user', content: String(args.message), tokens: 14, status: 'completed', createdAt: STAMP, metadata: null },
        { id: 'msg-answer', conversationId: CONVERSATION.id, role: 'assistant', content: ANSWER, tokens: 40, status: 'completed', createdAt: STAMP, metadata: JSON.stringify({ sources }) },
      ];
      return { conversationId: CONVERSATION.id, message: ANSWER, messages: state.messages, contextUsed: 2, sources: [], timingMetrics: null };
    },
  }));
  return { ...shell, state } satisfies ShellBackend & { state: typeof state };
}
