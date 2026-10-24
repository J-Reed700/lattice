import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { COMMAND_PLUGINS, type CommandName } from '@/shared/ipc/routes.generated';
import { diagnostics } from '@/utils/diagnostics';

import { chatApi } from './client';

const route = (command: CommandName) => `plugin:${COMMAND_PLUGINS[command]}|${command}`;

type Case = [label: string, call: () => Promise<unknown>, command: CommandName, args: Record<string, unknown>];

const source = { title: 'Roman roads', excerpt: 'Via Appia', relevanceScore: 0.8 };

const cases: Case[] = [
  ['createConversationTangent', () => chatApi.createConversationTangent({ parentConversationId: 'c1' } as never), 'create_conversation_tangent', { request: { parentConversationId: 'c1' } }],
  ['listConversationTangents', () => chatApi.listConversationTangents('c1'), 'list_conversation_tangents', { request: { conversationId: 'c1' } }],
  ['promoteConversationTangent', () => chatApi.promoteConversationTangent('t1'), 'promote_conversation_tangent', { request: { conversationId: 't1' } }],
  ['createConversation', () => chatApi.createConversation('Bread baking', 'Be brief'), 'create_conversation', { request: { title: 'Bread baking', systemPrompt: 'Be brief' } }],
  ['setConversationModelChoice', () => chatApi.setConversationModelChoice('c1', { kind: 'default' } as never), 'set_conversation_model_choice', { conversationId: 'c1', choice: { kind: 'default' } }],
  ['listConversations', () => chatApi.listConversations(20, 40), 'list_conversations', { query: { limit: 20, offset: 40 } }],
  ['getConversation', () => chatApi.getConversation('c1'), 'get_conversation', { request: { conversationId: 'c1' } }],
  ['getConversationMessages', () => chatApi.getConversationMessages('c1'), 'get_conversation_messages', { request: { conversationId: 'c1' } }],
  ['renameConversation', () => chatApi.renameConversation('c1', 'Cell division'), 'rename_conversation', { request: { conversationId: 'c1', newTitle: 'Cell division' } }],
  ['chatWithConversation', () => chatApi.chatWithConversation('c1', 'Why did Rome fall?', { webSearch: true } as never, 'r1', ['a.pdf'], ['d1'], ['i1']), 'chat_with_conversation', {
    conversationId: 'c1', message: 'Why did Rome fall?', requestId: 'r1', toolPreferences: { webSearch: true },
    attachmentNames: ['a.pdf'], attachmentDocumentIds: ['d1'], attachmentImageIds: ['i1'],
  }],
  ['attachMessageImages', () => chatApi.attachMessageImages('c1', ['/p/leaf.png']), 'attach_message_images', { conversationId: 'c1', paths: ['/p/leaf.png'] }],
  ['releaseMessageImages', () => chatApi.releaseMessageImages('c1', ['i1']), 'release_message_images', { conversationId: 'c1', imageIds: ['i1'] }],
  ['restageMessageImage', () => chatApi.restageMessageImage('c1', 'i1'), 'restage_message_image', { conversationId: 'c1', imageId: 'i1' }],
  ['deleteConversation', () => chatApi.deleteConversation('c1'), 'delete_conversation', { request: { conversationId: 'c1' } }],
  ['createConversationSpace', () => chatApi.createConversationSpace({ name: 'Botany' } as never), 'create_conversation_space', { request: { name: 'Botany' } }],
  ['listConversationSpaces', () => chatApi.listConversationSpaces(), 'list_conversation_spaces', {}],
  ['createJournal', () => chatApi.createJournal({ name: 'Kitchen log' } as never), 'create_journal', { request: { name: 'Kitchen log' } }],
  ['listJournals', () => chatApi.listJournals(), 'list_journals', {}],
  ['listConversationSpaceMembers', () => chatApi.listConversationSpaceMembers('s1'), 'list_conversation_space_members', { spaceId: 's1' }],
  ['upsertConversationSpaceMember', () => chatApi.upsertConversationSpaceMember({ spaceId: 's1' } as never), 'upsert_conversation_space_member', { request: { spaceId: 's1' } }],
  ['removeConversationSpaceMember', () => chatApi.removeConversationSpaceMember({ spaceId: 's1' } as never), 'remove_conversation_space_member', { request: { spaceId: 's1' } }],
  ['updateConversationSpace', () => chatApi.updateConversationSpace({ spaceId: 's1' } as never), 'update_conversation_space', { request: { spaceId: 's1' } }],
  ['archiveConversationSpace', () => chatApi.archiveConversationSpace({ spaceId: 's1' } as never), 'archive_conversation_space', { request: { spaceId: 's1' } }],
  ['updateJournal', () => chatApi.updateJournal({ journalId: 'j1' } as never), 'update_journal', { request: { journalId: 'j1' } }],
  ['archiveJournal', () => chatApi.archiveJournal({ journalId: 'j1' } as never), 'archive_journal', { request: { journalId: 'j1' } }],
  ['deleteJournal', () => chatApi.deleteJournal({ journalId: 'j1' } as never), 'delete_journal', { request: { journalId: 'j1' } }],
  ['moveConversationToSpace', () => chatApi.moveConversationToSpace({ conversationId: 'c1' } as never), 'move_conversation_to_space', { request: { conversationId: 'c1' } }],
  ['addConversationToJournal', () => chatApi.addConversationToJournal({ conversationId: 'c1' } as never), 'add_conversation_to_journal', { request: { conversationId: 'c1' } }],
  ['removeConversationFromJournal', () => chatApi.removeConversationFromJournal({ conversationId: 'c1' } as never), 'remove_conversation_from_journal', { request: { conversationId: 'c1' } }],
  ['setConversationSaved', () => chatApi.setConversationSaved({ conversationId: 'c1', value: true } as never), 'set_conversation_saved', { request: { conversationId: 'c1', value: true } }],
  ['setConversationBookmarked', () => chatApi.setConversationBookmarked({ conversationId: 'c1', value: true } as never), 'set_conversation_bookmarked', { request: { conversationId: 'c1', value: true } }],
  ['setConversationPinned', () => chatApi.setConversationPinned({ conversationId: 'c1', value: true } as never), 'set_conversation_pinned', { request: { conversationId: 'c1', value: true } }],
  ['setConversationArchived', () => chatApi.setConversationArchived({ conversationId: 'c1', value: true } as never), 'set_conversation_archived', { request: { conversationId: 'c1', value: true } }],
  ['deleteConversationMessage', () => chatApi.deleteConversationMessage({ messageId: 'm1' } as never), 'delete_conversation_message', { request: { messageId: 'm1' } }],
  ['bookmarkConversationMessage', () => chatApi.bookmarkConversationMessage({ messageId: 'm1' } as never), 'bookmark_conversation_message', { request: { messageId: 'm1' } }],
  ['unbookmarkConversationMessage', () => chatApi.unbookmarkConversationMessage({ messageId: 'm1' } as never), 'unbookmark_conversation_message', { request: { messageId: 'm1' } }],
  ['listMessageBookmarks with a query', () => chatApi.listMessageBookmarks({ limit: 5 } as never), 'list_message_bookmarks', { query: { limit: 5 } }],
  ['listMessageBookmarks without a query', () => chatApi.listMessageBookmarks(), 'list_message_bookmarks', { query: {} }],
  ['listConversationsExplorer with a query', () => chatApi.listConversationsExplorer({ spaceId: 's1' } as never), 'list_conversations_explorer', { query: { spaceId: 's1' } }],
  ['listConversationsExplorer without a query', () => chatApi.listConversationsExplorer(), 'list_conversations_explorer', { query: {} }],
  ['listJournalConversations', () => chatApi.listJournalConversations({ journalId: 'j1' } as never), 'list_journal_conversations', { query: { journalId: 'j1' } }],
  ['listJournalEntryPins', () => chatApi.listJournalEntryPins('j1'), 'list_journal_entry_pins', { journalSpaceId: 'j1' }],
  ['setJournalEntryPinned', () => chatApi.setJournalEntryPinned({ journalSpaceId: 'j1' } as never), 'set_journal_entry_pinned', { request: { journalSpaceId: 'j1' } }],
  ['listSpaceDocuments', () => chatApi.listSpaceDocuments('s1', null, 'photosynthesis', 10), 'list_space_documents', { spaceId: 's1', conversationId: null, query: 'photosynthesis', limit: 10 }],
  ['listConversationLinkedDocuments', () => chatApi.listConversationLinkedDocuments('c1'), 'list_conversation_linked_documents', { conversationId: 'c1' }],
  ['removeConversationLinkedDocument', () => chatApi.removeConversationLinkedDocument('c1', 'd1'), 'remove_conversation_linked_document', { conversationId: 'c1', documentId: 'd1' }],
  ['addConversationWebSource with options', () => chatApi.addConversationWebSource('c1', 'https://example.org/rome', source), 'add_conversation_web_source', { conversationId: 'c1', url: 'https://example.org/rome', ...source }],
  ['addConversationWebSource without options', () => chatApi.addConversationWebSource('c1', 'https://example.org/rome'), 'add_conversation_web_source', { conversationId: 'c1', url: 'https://example.org/rome', title: undefined, excerpt: undefined, relevanceScore: undefined }],
  ['listConversationWebSources', () => chatApi.listConversationWebSources('c1'), 'list_conversation_web_sources', { conversationId: 'c1' }],
  ['removeConversationWebSource', () => chatApi.removeConversationWebSource('c1', 'w1'), 'remove_conversation_web_source', { conversationId: 'c1', sourceId: 'w1' }],
  ['listDocumentSpaceMemberships', () => chatApi.listDocumentSpaceMemberships('d1'), 'list_document_space_memberships', { documentId: 'd1' }],
  ['setDocumentSpaceMembership', () => chatApi.setDocumentSpaceMembership('d1', 's1', true), 'set_document_space_membership', { documentId: 'd1', spaceId: 's1', assigned: true }],
  ['truncateConversationAfter (default exclusive)', () => chatApi.truncateConversationAfter('c1', 'm1'), 'truncate_conversation_after', { request: { conversationId: 'c1', messageId: 'm1', inclusive: false } }],
  ['truncateConversationAfter (inclusive)', () => chatApi.truncateConversationAfter('c1', 'm1', true), 'truncate_conversation_after', { request: { conversationId: 'c1', messageId: 'm1', inclusive: true } }],
  ['forkConversation', () => chatApi.forkConversation('c1', 'm3'), 'fork_conversation', { request: { conversationId: 'c1', upToMessageId: 'm3' } }],
  ['continueInNewConversation', () => chatApi.continueInNewConversation('c1'), 'continue_in_new_conversation', { request: { conversationId: 'c1' } }],
  ['regenerateResponse with a model', () => chatApi.regenerateResponse('c1', undefined, 'r2', { id: 'qwen' } as never), 'regenerate_response', { conversationId: 'c1', toolPreferences: undefined, requestId: 'r2', model: { id: 'qwen' } }],
  ['regenerateResponse without a model sends null', () => chatApi.regenerateResponse('c1'), 'regenerate_response', { conversationId: 'c1', toolPreferences: undefined, requestId: undefined, model: null }],
  ['compactConversation with a count', () => chatApi.compactConversation('c1', 6), 'compact_conversation', { request: { conversationId: 'c1', keepRecentMessages: 6 } }],
  ['compactConversation without a count sends null', () => chatApi.compactConversation('c1'), 'compact_conversation', { request: { conversationId: 'c1', keepRecentMessages: null } }],
  ['manageKnowledge', () => chatApi.manageKnowledge({ action: 'list' } as never), 'manage_knowledge', { request: { action: 'list' } }],
  ['getConversationMemory with history', () => chatApi.getConversationMemory('c1', true), 'get_conversation_memory', { request: { conversationId: 'c1', includeHistory: true } }],
  ['getConversationMemory without history sends null', () => chatApi.getConversationMemory('c1'), 'get_conversation_memory', { request: { conversationId: 'c1', includeHistory: null } }],
  ['setDocumentsSpaceMembership', () => chatApi.setDocumentsSpaceMembership(['d1', 'd2'], 's1', false), 'set_documents_space_membership', { documentIds: ['d1', 'd2'], spaceId: 's1', assigned: false }],
  ['addDocumentsToLibrary', () => chatApi.addDocumentsToLibrary(['d1']), 'add_documents_to_library', { documentIds: ['d1'] }],
];

describe('chatApi', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    diagnostics.clear();
  });

  it.each(cases)('%s invokes its command through the registering plugin', async (_label, call, command, args) => {
    vi.mocked(invoke).mockResolvedValueOnce({ echoed: command });
    const result = await call();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith(route(command), args);
    expect(result).toEqual({ ok: true, data: { echoed: command } });
  });

  it('returns a backend failure as a failed result instead of throwing', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('Conversation not found: c9');
    const result = await chatApi.getConversation('c9');
    expect(result).toMatchObject({ ok: false, error: 'Conversation not found: c9' });
  });

  describe('cancelConversationGeneration', () => {
    it('sends a cancel-only chat request and drops the chat payload on success', async () => {
      vi.mocked(invoke).mockResolvedValueOnce({ answer: 'partial' });
      const result = await chatApi.cancelConversationGeneration('c1', 'r1');
      expect(invoke).toHaveBeenCalledWith(route('chat_with_conversation'), {
        conversationId: 'c1', message: '', requestId: 'r1', cancelOnly: true,
      });
      expect(result).toEqual({ ok: true, data: undefined });
    });

    it('passes the failure through when the cancel is refused', async () => {
      vi.mocked(invoke).mockRejectedValueOnce('No generation running');
      const result = await chatApi.cancelConversationGeneration('c1', 'r1');
      expect(result).toMatchObject({ ok: false, error: 'No generation running' });
    });
  });
});
