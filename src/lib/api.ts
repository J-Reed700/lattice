/**
 * VaultAPI - Type-safe wrapper around Tauri IPC commands
 *
 * This module provides a clean interface to all Tauri backend commands
 * with proper TypeScript types and error handling.
 */

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

// Enhanced API Result types
import { parseApiError } from './errorHandling';

import type * as Wire from './bindings';
import type {
  SearchOptions,
  SearchResult,
  IndexStatus,
  IndexingStats,
  IndexedFolder,
  IndexingActivity,
  IndexFileResponse,
  RecentDocument,
  DocumentMetadata,
  ApiResult,
  WebIngestResponse,
  UrlPreview,
  CleanArticle,
  // API types
  AppSettings,
  TestCustomToolRequest,
  TestCustomToolResponse,
  TestOllamaConnectionRequest,
  TestOllamaConnectionResponse,
  DailyNote,
  WorkspaceNote,
  ListWorkspaceNotesResponse,
  Tag,
  FavoriteDocument,
  // Tag Request/Response types
  ListTagsResponse,
  RemoveTagFromDocumentRequest,
  GetDocumentTagsRequest,
  DocumentTagsResponse,
  QAResponse,
  LLMHealthStatus,
  CacheMetrics,
  HealthStatus,
  SystemStats,
  Mention,
  ExtractMentionsResponse,
  SearchMentionsResponse,
  BacklinksResponse,
  FileMetadata,
  UpdateInfo,
  EmbeddingModelInfo,
  ParsedLinksResponse,
  ExtractAndResolveLinksResponse,
  OpenFileResponseDto,
  // Conversation types (Wave 2B)
  Conversation,
  ToolPreferences,
  GetConversationMessagesResponse,
  CreateConversationResponse,
  ListConversationsResponse,
  GetConversationResponse,
  RenameConversationResponse,
  DeleteConversationResponse,
  ConversationSpaceDto,
  ConversationJournalDto,
  ConversationSpaceMemberDto,
  CreateConversationSpaceRequest,
  CreateConversationJournalRequest,
  UpdateConversationSpaceRequest,
  UpdateConversationJournalRequest,
  ArchiveConversationSpaceRequest,
  ArchiveConversationJournalRequest,
  DeleteConversationJournalRequest,
  MoveConversationToSpaceRequest,
  AddConversationToJournalRequest,
  RemoveConversationFromJournalRequest,
  UpsertConversationSpaceMemberRequest,
  RemoveConversationSpaceMemberRequest,
  SetConversationStateRequest,
  ListConversationsExplorerQuery,
  ListJournalConversationsQuery,
  SynthesizeJournalEntriesRequest,
  SynthesizeJournalEntriesResponse,
  DeleteConversationMessageRequest,
  BookmarkConversationMessageRequest,
  UnbookmarkConversationMessageRequest,
  ListMessageBookmarksQuery,
  ListMessageBookmarksResponse,
  ConversationLinkedDocumentDto,
  ConversationWebSourceDto,
  DocumentSpaceMembershipDto,
  SpaceDocument,
  // Function calling types (Wave 2B)
  FunctionDefinition,
  FunctionResult,
  // Backup and export types (Wave 2D)
  // Web content types (Wave 2D)
  // Batch and credentials types (Wave 3)
  BatchJobSummary,
  BatchJobStatus,
  HfTokenStatus,
  IndexingSnapshot,
  BackupInfo,
  CreateBackupResult,
  RestoreBackupResult,
  ExportSummary,
  // Off-device backup (encrypted archive)
  ArchiveStatus,
  ArchiveSetup,
  ArchiveRun,
  RestoreArchiveResult,
  WordConfirmation,
  VersionInfo,
  // Model management types (Wave 4B)
  SystemCapabilities,
  ModelSearchResult,
  ModelRecommendation,
  ModelCatalogCacheStats,
  // Corpus shape types
  CorpusShapeDto,
  CitingConversationDto,
  SimilarDocumentDto,
  ClusterDto,
  ClusterRunDto,
  QuickCaptureResultDto,
  GetMentionsForDocumentResult,
} from '../types';
import type { ChatStarters } from '../types/api/chatStarters';
import type {
  CompareDocumentsRequest,
  CompareTableDto,
} from '../types/api/compare';
import type {
  CreatePassageReferenceRequest,
  PassageReferenceDto,
} from '../types/api/passageReferences';
import type { UpdatePassageReferenceRequest } from '../types/api/references';
import type { DownloadModelResponse } from '../types/download';
import type { DownloadedModel } from '../types/downloadedModels';
import type { StartDownloadRequest, DownloadStatus } from '../types/downloads';
import type { SearchModelCatalogRequest } from '../types/modelCatalog';
import type { Transcript, TranscriptionStatus } from '../types/transcription';

/**
 * Gateway Pattern - Command to Domain mapping
 * Maps old command names to their new domain + command structure
 */
const COMMAND_DOMAIN_MAP: Record<string, { domain: string; command: string }> = {
  // Model domain
  get_models_with_metadata: { domain: 'model', command: 'list_downloaded_models' },
  is_model_already_downloaded: { domain: 'model', command: 'is_model_already_downloaded' },
  set_active_chat_model: { domain: 'model', command: 'set_active_chat_model' },
  warm_up_active_chat_model: { domain: 'model', command: 'warm_up_active_chat_model' },
  warm_up_active_utility_model: { domain: 'model', command: 'warm_up_active_utility_model' },
  get_active_chat_model: { domain: 'model', command: 'get_active_chat_model' },
  get_active_models: { domain: 'model', command: 'get_active_models' },
  delete_downloaded_model_and_file: { domain: 'model', command: 'delete_model' },
  get_active_embedding_model: { domain: 'model', command: 'get_active_embedding_model' },
  set_active_embedding_model: { domain: 'model', command: 'set_active_embedding_model' },
  detect_system_capabilities: { domain: 'model', command: 'detect_system_capabilities' },
  get_all_recommended_models: { domain: 'model', command: 'get_all_recommended_models' },
  get_model_catalog_stats: { domain: 'model', command: 'get_model_catalog_stats' },
  get_model_download_path: { domain: 'model', command: 'get_model_download_path' },

  // Search domain
  search_documents: { domain: 'search', command: 'search_documents' },
  search_fast: { domain: 'search', command: 'search_fast' },
  semantic_search: { domain: 'search', command: 'semantic_search' },
  hybrid_search: { domain: 'search', command: 'hybrid_search' },
  find_similar: { domain: 'search', command: 'find_similar' },
  search_with_recency: { domain: 'search', command: 'search_with_recency' },
  batch_search: { domain: 'search', command: 'batch_search' },
  reranker_status: { domain: 'search', command: 'reranker_status' },
  download_reranker: { domain: 'search', command: 'download_reranker' },

  // File domain
  open_file: { domain: 'file', command: 'open_file' },
  open_file_by_id: { domain: 'file', command: 'open_file_by_id' },
  get_file_path_by_id: { domain: 'file', command: 'get_file_path_by_id' },
  get_file_metadata: { domain: 'file', command: 'get_file_metadata' },
  read_file_content: { domain: 'file', command: 'read_file_content' },
  read_file_bytes: { domain: 'file', command: 'read_file_bytes' },
  show_in_folder: { domain: 'file', command: 'show_in_folder' },
  get_indexed_folders: { domain: 'file', command: 'get_indexed_folders' },
  get_indexing_activities: { domain: 'file', command: 'get_indexing_activities' },

  // Health domain
  health_check: { domain: 'health', command: 'health_check' },
  get_system_stats: { domain: 'health', command: 'get_system_stats' },
  get_version: { domain: 'health', command: 'get_version' },

  // Database/initialization domain (routes to health for now)
  initialize_database: { domain: 'health', command: 'initialize_database' },

  // Indexing domain (routes to file)
  get_indexing_stats: { domain: 'file', command: 'get_indexing_stats' },
  list_all_documents: { domain: 'file', command: 'list_all_documents' },
  index_directory: { domain: 'file', command: 'index_directory' },
  index_file: { domain: 'file', command: 'index_file' },
  reindex_file: { domain: 'file', command: 'reindex_file' },
  remove_indexed_file: { domain: 'file', command: 'remove_indexed_file' },
  delete_document: { domain: 'file', command: 'delete_document' },
  get_index_progress: { domain: 'file', command: 'get_index_progress' },
  cancel_indexing: { domain: 'file', command: 'cancel_indexing' },
  pause_indexing: { domain: 'file', command: 'pause_indexing' },
  resume_indexing: { domain: 'file', command: 'resume_indexing' },
  clear_indexing_failure: { domain: 'file', command: 'clear_indexing_failure' },
  get_indexing_status: { domain: 'file', command: 'get_indexing_status' },
  list_indexed_files: { domain: 'file', command: 'list_indexed_files' },
  get_recent_documents: { domain: 'file', command: 'get_recent_documents' },
  get_document: { domain: 'file', command: 'get_document' },

  // Tag domain
  get_all_tags_with_counts: { domain: 'tags', command: 'get_all_tags_with_counts' },
  remove_tag_from_document: { domain: 'tags', command: 'remove_tag_from_document' },
  get_document_tags: { domain: 'tags', command: 'get_document_tags' },
  apply_tags: { domain: 'tags', command: 'apply_tags' },
  generate_tags_for_document: { domain: 'tags', command: 'generate_tags_for_document' },

  // QA domain
  ask_question: { domain: 'qa', command: 'ask_question_wrapper' },
  ask_question_stream: { domain: 'qa', command: 'ask_question_stream_wrapper' },
  check_llm_health: { domain: 'qa', command: 'check_llm_health_wrapper' },
  generate_chat_starters: { domain: 'qa', command: 'generate_chat_starters_wrapper' },

  // Conversation domain
  create_conversation: { domain: 'conversation', command: 'create_conversation' },
  list_conversations: { domain: 'conversation', command: 'list_conversations' },
  get_conversation: { domain: 'conversation', command: 'get_conversation' },
  get_conversation_messages: { domain: 'conversation', command: 'get_conversation_messages' },
  rename_conversation: { domain: 'conversation', command: 'rename_conversation' },
  chat_with_conversation: { domain: 'conversation', command: 'chat_with_conversation' },
  delete_conversation: { domain: 'conversation', command: 'delete_conversation' },
  create_conversation_space: { domain: 'conversation', command: 'create_conversation_space' },
  list_conversation_spaces: { domain: 'conversation', command: 'list_conversation_spaces' },
  create_journal: { domain: 'conversation', command: 'create_journal' },
  list_journals: { domain: 'conversation', command: 'list_journals' },
  list_conversation_space_members: { domain: 'conversation', command: 'list_conversation_space_members' },
  upsert_conversation_space_member: { domain: 'conversation', command: 'upsert_conversation_space_member' },
  remove_conversation_space_member: { domain: 'conversation', command: 'remove_conversation_space_member' },
  update_conversation_space: { domain: 'conversation', command: 'update_conversation_space' },
  archive_conversation_space: { domain: 'conversation', command: 'archive_conversation_space' },
  update_journal: { domain: 'conversation', command: 'update_journal' },
  archive_journal: { domain: 'conversation', command: 'archive_journal' },
  delete_journal: { domain: 'conversation', command: 'delete_journal' },
  move_conversation_to_space: { domain: 'conversation', command: 'move_conversation_to_space' },
  add_conversation_to_journal: { domain: 'conversation', command: 'add_conversation_to_journal' },
  remove_conversation_from_journal: { domain: 'conversation', command: 'remove_conversation_from_journal' },
  set_conversation_saved: { domain: 'conversation', command: 'set_conversation_saved' },
  set_conversation_bookmarked: { domain: 'conversation', command: 'set_conversation_bookmarked' },
  set_conversation_pinned: { domain: 'conversation', command: 'set_conversation_pinned' },
  set_conversation_archived: { domain: 'conversation', command: 'set_conversation_archived' },
  delete_conversation_message: { domain: 'conversation', command: 'delete_conversation_message' },
  bookmark_conversation_message: { domain: 'conversation', command: 'bookmark_conversation_message' },
  unbookmark_conversation_message: { domain: 'conversation', command: 'unbookmark_conversation_message' },
  list_message_bookmarks: { domain: 'conversation', command: 'list_message_bookmarks' },
  list_conversations_explorer: { domain: 'conversation', command: 'list_conversations_explorer' },
  list_journal_conversations: { domain: 'conversation', command: 'list_journal_conversations' },
  list_space_documents: { domain: 'conversation', command: 'list_space_documents' },
  list_conversation_linked_documents: { domain: 'conversation', command: 'list_conversation_linked_documents' },
  remove_conversation_linked_document: { domain: 'conversation', command: 'remove_conversation_linked_document' },
  add_conversation_web_source: { domain: 'conversation', command: 'add_conversation_web_source' },
  list_conversation_web_sources: { domain: 'conversation', command: 'list_conversation_web_sources' },
  remove_conversation_web_source: { domain: 'conversation', command: 'remove_conversation_web_source' },
  list_document_space_memberships: { domain: 'conversation', command: 'list_document_space_memberships' },
  set_document_space_membership: { domain: 'conversation', command: 'set_document_space_membership' },
  set_documents_space_membership: { domain: 'conversation', command: 'set_documents_space_membership' },
  synthesize_journal_entries: { domain: 'conversation', command: 'synthesize_journal_entries' },
  truncate_conversation_after: { domain: 'conversation', command: 'truncate_conversation_after' },
  fork_conversation: { domain: 'conversation', command: 'fork_conversation' },
  regenerate_response: { domain: 'conversation', command: 'regenerate_response' },
  compact_conversation: { domain: 'conversation', command: 'compact_conversation' },

  // Passage references
  create_passage_reference: { domain: 'references', command: 'create_passage_reference' },
  list_passage_references: { domain: 'references', command: 'list_passage_references' },
  update_passage_reference: { domain: 'references', command: 'update_passage_reference' },
  delete_passage_reference: { domain: 'references', command: 'delete_passage_reference' },

  // Compare domain
  list_study_decks: { domain: 'study', command: 'list_study_decks' },
  get_study_deck: { domain: 'study', command: 'get_study_deck' },
  generate_study_deck: { domain: 'study', command: 'generate_study_deck' },
  generate_conversation_study_deck: { domain: 'study', command: 'generate_conversation_study_deck' },
  review_study_card: { domain: 'study', command: 'review_study_card' },
  update_study_card: { domain: 'study', command: 'update_study_card' },
  delete_study_deck: { domain: 'study', command: 'delete_study_deck' },
  compare_documents: { domain: 'compare', command: 'compare_documents' },

  // Credentials domain
  set_huggingface_token: { domain: 'huggingface', command: 'set_huggingface_token' },
  get_huggingface_token_status: { domain: 'huggingface', command: 'get_huggingface_token_status' },
  get_huggingface_token: { domain: 'huggingface', command: 'get_huggingface_token' },
  delete_huggingface_token: { domain: 'huggingface', command: 'delete_huggingface_token' },

  // Batch operations domain
  get_batch_history: { domain: 'batch', command: 'get_batch_history' },
  delete_batch_job: { domain: 'batch', command: 'delete_batch_job' },
  retry_failed_items: { domain: 'batch', command: 'retry_failed_items' },
  get_batch_status: { domain: 'batch', command: 'get_batch_status' },
  cancel_batch: { domain: 'batch', command: 'cancel_batch' },
  batch_import_files: { domain: 'batch', command: 'batch_import_files' },
  batch_import_urls: { domain: 'batch', command: 'batch_import_urls' },

  // Download management domain
  start_model_download: { domain: 'download', command: 'start_model_download' },
  pause_download: { domain: 'download', command: 'pause_download' },
  resume_download: { domain: 'download', command: 'resume_download' },
  cancel_download: { domain: 'download', command: 'download_cancel' },
  retry_download: { domain: 'download', command: 'retry_download' },
  remove_download: { domain: 'download', command: 'remove_download' },
  clear_completed_downloads: { domain: 'download', command: 'clear_completed_downloads' },
  list_downloads: { domain: 'download', command: 'list_downloads' },
  get_download_status: { domain: 'download', command: 'download_get_status' },

  // Additional model management
  download_model: { domain: 'model', command: 'download_model' },
  get_compatible_models: { domain: 'model', command: 'get_compatible_models' },
  search_model_catalog: { domain: 'model', command: 'search_model_catalog' },
  refresh_model_catalog: { domain: 'model', command: 'refresh_model_catalog' },
  clear_model_catalog_cache: { domain: 'model', command: 'clear_model_catalog_cache' },
  clear_active_chat_model: { domain: 'model', command: 'clear_active_chat_model' },
  clear_active_embedding_model: { domain: 'model', command: 'clear_active_embedding_model' },
  set_active_utility_model: { domain: 'model', command: 'set_active_utility_model' },
  clear_active_utility_model: { domain: 'model', command: 'clear_active_utility_model' },
  delete_model: { domain: 'model', command: 'delete_model' },

  // File operations - additional
  rename_document: { domain: 'file', command: 'rename_document' },

  // Cache management
  clear_search_cache: { domain: 'cache', command: 'clear_search_cache' },
  clear_cache: { domain: 'cache', command: 'clear_cache' },
  get_cache_stats: { domain: 'cache', command: 'get_cache_stats' },
  get_cache_metrics: { domain: 'cache', command: 'get_cache_metrics' },

  // Additional commands for remaining APIs
  add_favorite: { domain: 'favorites', command: 'add_favorite' },
  remove_favorite: { domain: 'favorites', command: 'remove_favorite' },
  get_favorites: { domain: 'favorites', command: 'get_favorites' },
  is_favorite: { domain: 'favorites', command: 'is_favorite' },
  extract_mentions: { domain: 'mention', command: 'extract_mentions' },
  search_mentions: { domain: 'mention', command: 'search_mentions' },
  get_mentions_for_document: { domain: 'mention', command: 'get_mentions_for_document' },
  get_backlinks_for_mention: { domain: 'mention', command: 'get_backlinks_for_mention' },
  get_mentions_by_type: { domain: 'mention', command: 'get_mentions_by_type' },
  create_mention: { domain: 'mention', command: 'create_mention' },
  delete_mention: { domain: 'mention', command: 'delete_mention' },
  generate_embedding: { domain: 'embeddings', command: 'generate_embedding' },
  generate_embeddings_batch: { domain: 'embeddings', command: 'generate_embeddings_batch' },
  get_embedding_model_info: { domain: 'embeddings', command: 'get_embedding_model_info' },
  parse_wikilinks: { domain: 'extraction', command: 'parse_wikilinks' },
  extract_document_title: { domain: 'extraction', command: 'extract_document_title' },
  resolve_wikilink: { domain: 'extraction', command: 'resolve_wikilink' },
  extract_and_resolve_links: { domain: 'extraction', command: 'extract_and_resolve_links' },
  get_settings: { domain: 'settings', command: 'get_settings' },
  get_settings_category: { domain: 'settings', command: 'get_settings_category' },
  update_settings: { domain: 'settings', command: 'update_settings' },
  reset_settings: { domain: 'settings', command: 'reset_settings' },
  export_settings: { domain: 'settings', command: 'export_settings' },
  import_settings: { domain: 'settings', command: 'import_settings' },
  get_system_theme: { domain: 'settings', command: 'get_system_theme' },
  validate_folder_path: { domain: 'settings', command: 'validate_folder_path' },
  test_ollama_connection: { domain: 'settings', command: 'test_ollama_connection' },
  test_llama_cpp_connection: { domain: 'settings', command: 'test_llama_cpp_connection' },
  test_custom_tool: { domain: 'settings', command: 'test_custom_tool' },
  get_today_note: { domain: 'dailynotes', command: 'get_today_note' },
  quick_capture: { domain: 'dailynotes', command: 'quick_capture' },
  get_daily_notes_range: { domain: 'dailynotes', command: 'get_daily_notes_range' },
  get_previous_daily_note: { domain: 'dailynotes', command: 'get_previous_daily_note' },
  get_next_daily_note: { domain: 'dailynotes', command: 'get_next_daily_note' },
  update_daily_note_content: { domain: 'dailynotes', command: 'update_daily_note_content' },
  list_workspace_notes: { domain: 'dailynotes', command: 'list_workspace_notes' },
  create_workspace_note: { domain: 'dailynotes', command: 'create_workspace_note' },
  update_workspace_note: { domain: 'dailynotes', command: 'update_workspace_note' },
  delete_workspace_note: { domain: 'dailynotes', command: 'delete_workspace_note' },
  add_watch_folder: { domain: 'settings', command: 'add_watch_folder' },
  remove_watch_folder: { domain: 'settings', command: 'remove_watch_folder' },
  ingest_web_url: { domain: 'web', command: 'ingest_web_url' },
  fetch_url_preview: { domain: 'web', command: 'fetch_url_preview' },
  extract_article: { domain: 'web', command: 'extract_article' },
  check_for_updates: { domain: 'updates', command: 'check_for_updates' },
  execute_function: { domain: 'functions', command: 'execute_function' },
  list_available_functions: { domain: 'functions', command: 'list_available_functions' },
  get_function_stats: { domain: 'functions', command: 'get_function_stats' },
  get_version_info: { domain: 'updates', command: 'get_version_info' },
  rescan_vault: { domain: 'vault', command: 'rescan_vault' },

  // Transcription
  transcribe_file: { domain: 'transcription', command: 'transcribe_file' },
  get_transcription_status: { domain: 'transcription', command: 'get_transcription_status' },

  // Corpus shape
  get_corpus_shape: { domain: 'file', command: 'get_corpus_shape' },
  list_conversations_citing_document: { domain: 'file', command: 'list_conversations_citing_document' },
  find_similar_documents: { domain: 'search', command: 'find_similar_documents' },
  cluster_vault_run: { domain: 'corpus-shape', command: 'cluster_vault_run' },
  cluster_vault_debug: { domain: 'corpus-shape', command: 'cluster_vault_debug' },
  list_clusters: { domain: 'corpus-shape', command: 'list_clusters' },

  // Backup and export domain
  plugin_create_backup: { domain: 'backup', command: 'plugin_create_backup' },
  plugin_restore_backup: { domain: 'backup', command: 'plugin_restore_backup' },
  plugin_list_backups: { domain: 'backup', command: 'plugin_list_backups' },
  plugin_export_markdown: { domain: 'backup', command: 'plugin_export_markdown' },
  plugin_export_json: { domain: 'backup', command: 'plugin_export_json' },
  plugin_export_csv: { domain: 'backup', command: 'plugin_export_csv' },
  plugin_export_html: { domain: 'backup', command: 'plugin_export_html' },

  // Off-device backup (encrypted archive) domain
  plugin_get_archive_status: { domain: 'backup', command: 'plugin_get_archive_status' },
  plugin_begin_archive_setup: { domain: 'backup', command: 'plugin_begin_archive_setup' },
  plugin_confirm_archive_setup: { domain: 'backup', command: 'plugin_confirm_archive_setup' },
  plugin_choose_archive_destination: { domain: 'backup', command: 'plugin_choose_archive_destination' },
  plugin_set_archive_keep_count: { domain: 'backup', command: 'plugin_set_archive_keep_count' },
  plugin_set_archive_passphrase: { domain: 'backup', command: 'plugin_set_archive_passphrase' },
  plugin_rotate_recovery_code: { domain: 'backup', command: 'plugin_rotate_recovery_code' },
  plugin_disable_archive: { domain: 'backup', command: 'plugin_disable_archive' },
  plugin_create_archive_now: { domain: 'backup', command: 'plugin_create_archive_now' },
  plugin_restore_archive: { domain: 'backup', command: 'plugin_restore_archive' },
};

/**
 * Wrap a Tauri command invocation with ApiResult type
 * Routes through Plugin Pattern using plugin:domain|command syntax
 */
function shouldRetryCommandRoute(error: unknown): boolean {
  const message = String(error).toLowerCase();
  return (
    message.includes('not allowed') ||
    message.includes('command not found') ||
    message.includes('unknown command') ||
    message.includes('not found')
  );
}

async function invokeCommandWithFallback<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  domain: string,
  pluginCommand: string
): Promise<T> {
  const attempts = [
    `plugin:${domain}|${pluginCommand}`,
    `${domain}.${pluginCommand}`,
    pluginCommand,
  ];

  let lastError: unknown = null;
  for (let idx = 0; idx < attempts.length; idx += 1) {
    const signature = attempts[idx];
    try {
      return await invoke<T>(signature, args || {});
    } catch (error) {
      lastError = error;
      const canRetry = idx < attempts.length - 1 && shouldRetryCommandRoute(error);
      if (!canRetry) {
        throw error;
      }
      console.warn(`[API] Falling back from '${signature}' for command '${command}':`, error);
    }
  }

  throw lastError;
}

async function apiCall<T>(command: string, args?: Record<string, unknown>): Promise<ApiResult<T>> {
  const pluginRoute = COMMAND_DOMAIN_MAP[command];
  if (!pluginRoute) {
    // A missing route is a programming error, not a backend failure: every
    // command must be declared in COMMAND_DOMAIN_MAP so `contracts:check` can
    // verify it against the generated bindings.
    throw new Error(`[API] Command '${command}' has no COMMAND_DOMAIN_MAP entry`);
  }

  try {
    const data = await invokeCommandWithFallback<T>(
      command,
      args,
      pluginRoute.domain,
      pluginRoute.command
    );

    return { ok: true, data };
  } catch (error) {
    const apiError = parseApiError(error);
    return {
      ok: false,
      error: apiError.message,
      details: apiError
    };
  }
}

function normalizeDownloadStatus(raw: Wire.DownloadStatusResponse): DownloadStatus {
  switch (raw.state) {
    case 'Pending': case 'Downloading': case 'Paused': case 'Completed': case 'Failed': case 'Cancelled':
      return { ...raw, state: raw.state };
    default:
      throw new Error(`Unknown download state: ${raw.state}`);
  }
}

function computeBatchProgress(totalItems: number, completedItems: number, failedItems: number): number {
  if (totalItems <= 0) {
    return 0;
  }
  const processed = completedItems + failedItems;
  return Math.min(100, Math.max(0, (processed / totalItems) * 100));
}

function normalizeBatchSummary(raw: Wire.BatchJobSummaryDto): BatchJobSummary {
  return {
    ...raw,
    id: raw.jobId,
    progress: computeBatchProgress(raw.totalItems, raw.completedItems, raw.failedItems),
  };
}

function normalizeBatchStatus(raw: Wire.BatchJobStatusDto): BatchJobStatus {
  return {
    ...raw,
    id: raw.jobId,
    total_items: raw.totalItems,
    completed_items: raw.completedItems,
    failed_items: raw.failedItems,
    progress: computeBatchProgress(raw.totalItems, raw.completedItems, raw.failedItems),
    completedAt: raw.completedAt ?? null,
    items: raw.items.map(item => ({ ...item, id: item.itemId, url: item.target })),
  };
}

function unwrapNestedApiResult<T>(
  response: ApiResult<unknown>,
  fallbackError: string
): ApiResult<T> {
  if (!response.ok) {
    return response as unknown as ApiResult<T>;
  }

  const payload = response.data as Record<string, unknown> | null;
  if (
    payload &&
    typeof payload === 'object' &&
    Object.prototype.hasOwnProperty.call(payload, 'ok') &&
    typeof payload.ok === 'boolean'
  ) {
    if (payload.ok === true) {
      return { ok: true, data: payload.data as T };
    }

    const nestedError = (payload.error ?? {}) as Record<string, unknown>;
    const nestedMessage =
      typeof nestedError.message === 'string' ? nestedError.message : fallbackError;
    return {
      ok: false,
      error: nestedMessage,
      details: {
        code: typeof nestedError.code === 'string' ? nestedError.code : 'UNKNOWN',
        message: nestedMessage,
        details: nestedError,
      },
    };
  }

  return { ok: true, data: response.data as T };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function normalizeSearchResult(raw: unknown): SearchResult | null {
  if (!isRecord(raw) || typeof raw.id !== 'string' || !raw.id
    || typeof raw.title !== 'string' || typeof raw.content !== 'string'
    || typeof raw.score !== 'number' || !Number.isFinite(raw.score)) {
    return null;
  }

  const { id, title, content, score } = raw;

  return {
    id,
    title,
    content,
    score,
    path: (raw.path as string | null | undefined) ?? null,
    documentId:
      (raw.documentId as string | null | undefined) ??
      (raw.document_id as string | null | undefined) ??
      null,
    position: (raw.position as number | null | undefined) ?? null,
    vectorScore:
      (raw.vectorScore as number | null | undefined) ??
      (raw.vector_score as number | null | undefined) ??
      null,
    bm25Score:
      (raw.bm25Score as number | null | undefined) ??
      (raw.bm25_score as number | null | undefined) ??
      null,
    vectorRank:
      (raw.vectorRank as number | null | undefined) ??
      (raw.vector_rank as number | null | undefined) ??
      null,
    bm25Rank:
      (raw.bm25Rank as number | null | undefined) ??
      (raw.bm25_rank as number | null | undefined) ??
      null,
    metadata: (raw.metadata as Record<string, unknown> | undefined) ?? {},
    highlights: Array.isArray(raw.highlights)
      ? (raw.highlights as string[])
      : undefined,
  };
}

function extractSearchResults(payload: unknown): SearchResult[] | null {
  if (Array.isArray(payload)) {
    const results = payload.map(normalizeSearchResult);
    return results.every((item): item is SearchResult => item !== null) ? results : null;
  }

  if (!isRecord(payload)) {
    return null;
  }

  if (payload.ok === true && 'data' in payload) {
    return extractSearchResults(payload.data);
  }

  if (Array.isArray(payload.results)) {
    return extractSearchResults(payload.results);
  }

  if (Array.isArray(payload.data)) {
    return extractSearchResults(payload.data);
  }

  return null;
}

function unwrapSearchResults(
  response: ApiResult<unknown>,
  fallbackError: string
): ApiResult<SearchResult[]> {
  const unwrapped = unwrapNestedApiResult<unknown>(response, fallbackError);
  if (!unwrapped.ok) {
    return unwrapped;
  }

  const data = extractSearchResults(unwrapped.data);
  return data === null ? { ok: false, error: `${fallbackError}: invalid search response` } : { ok: true, data };
}

type BackendIndexProgress = Wire.IndexProgress;

function normalizeIndexProgress(raw: BackendIndexProgress): IndexingSnapshot {
  const totalFiles = raw.total_files ?? 0;
  const processed = raw.files_processed ?? 0;
  const failed = raw.failed ?? 0;
  const percentage =
    raw.percent_complete ?? (totalFiles > 0 ? (processed / totalFiles) * 100 : 0);

  const validStatuses: ReadonlySet<string> = new Set(['idle', 'scanning', 'processing', 'complete', 'error', 'cancelled']);
  if (!validStatuses.has(raw.status)) {
    throw new Error(`Unknown indexing status: ${raw.status}`);
  }
  const status = raw.status as IndexStatus;

  return {
    totalFiles,
    processed,
    failed,
    currentFile: raw.current_file ?? undefined,
    status,
    percentage,
    paused: raw.paused ?? false,
    failures: raw.failures ?? [],
  };
}

function normalizeIndexingStats(raw: IndexingStats): IndexingStats {
  const indexedDocuments =
    raw.indexedDocuments ?? (raw as { indexed_documents?: number }).indexed_documents ?? 0;
  const totalChunks =
    raw.totalChunks ?? (raw as { total_chunks?: number }).total_chunks ?? 0;

  return {
    indexedDocuments,
    totalChunks,
  };
}

/**
 * VaultAPI - Type-safe interface to all Tauri backend commands.
 *
 * Provides a comprehensive API for interacting with the Recall desktop app backend,
 * including search, indexing, file management, configuration, and more.
 * All methods return ApiResult<T> for consistent error handling.
 */
const VaultAPI = {

  /**
   * Initializes the SQLite database with required schema and tables.
   * Creates all necessary tables for documents, embeddings, tags, and metadata.
   * Should be called once on application startup.
   *
   * @returns Success message confirming database initialization
   */
  initializeDatabase: async (): Promise<ApiResult<string>> => apiCall<string>('initialize_database'),


  /**
   * Searches indexed documents using configurable search options.
   * Supports semantic search via embeddings, with optional filters and ranking.
   *
   * @param options - Search configuration including query, limit, filters, and mode
   * @returns Array of search results with content, metadata, and similarity scores
   *
   * @example
   * const result = await VaultAPI.searchDocuments({
   *   query: 'machine learning',
   *   limit: 10,
   *   filters: { tags: ['ai'] }
   * });
   */
  searchDocuments: async (options: SearchOptions): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_documents', { options });
    return unwrapSearchResults(raw, 'Document search failed');
  },

  /**
   * Performs recency-aware search that boosts recent documents.
   * Combines relevance scoring with time-based weighting.
   *
   * @param options - Recency search options with query, limit, weights
   * @returns Search results with recency boost applied
   *
   * @example
   * const result = await VaultAPI.searchWithRecency({
   *   query: 'recent updates',
   *   limit: 10,
   *   recencyWeight: 0.3,
   *   maxAgeDays: 30
   * });
   */
  searchWithRecency: async (options: {
    query: string;
    limit?: number;
    recencyWeight?: number;
    maxAgeDays?: number;
  }): Promise<ApiResult<SearchResult[]>> => {
    const result = await apiCall<Wire.SearchResultDto[]>('search_with_recency', { options });
    return unwrapSearchResults(result, 'Recency search failed');
  },

  /**
   * Performs batch search for multiple queries efficiently.
   * Processes all queries in parallel and returns results in order.
   *
   * @param queries - Array of search query strings
   * @param limit - Maximum results per query (default: 10)
   * @param searchMode - Search algorithm: 'vector', 'keyword', or 'hybrid'
   * @returns Array of result arrays, one for each query
   *
   * @example
   * const results = await VaultAPI.batchSearch(
   *   ['machine learning', 'neural networks', 'deep learning'],
   *   5,
   *   'hybrid'
   * );
   * // results[0] = results for 'machine learning'
   * // results[1] = results for 'neural networks'
   * // results[2] = results for 'deep learning'
   */
  batchSearch: async (
    queries: string[],
    limit?: number,
    searchMode?: string
  ): Promise<ApiResult<SearchResult[][]>> => {
    const result = await apiCall<Wire.SearchResultDto[][]>('batch_search', { queries, limit, searchMode });
    if (!result.ok) return result;
    const groups: SearchResult[][] = [];
    for (const raw of result.data) {
      const group = unwrapSearchResults({ ok: true, data: raw }, 'Batch search failed');
      if (!group.ok) return group;
      groups.push(group.data);
    }
    return { ok: true, data: groups };
  },

  /**
   * Performs fast full-text search without semantic embeddings.
   * Uses SQLite FTS5 for keyword matching. Ideal for simple queries.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching documents ranked by relevance
   */
  searchFast: async (query: string, limit?: number): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_fast', { query, limit: limit ?? 10 });
    return unwrapSearchResults(raw, 'Keyword search failed');
  },

  /**
   * Executes hybrid search combining semantic and keyword matching.
   * Merges results from embeddings and FTS5, reranked by combined score.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (optional)
   * @param searchMode - Search strategy: 'semantic', 'keyword', or 'hybrid' (default)
   * @returns Unified array of search results with combined relevance scores
   */
  searchHybrid: async (
    query: string,
    limit?: number,
    searchMode?: 'semantic' | 'keyword' | 'hybrid'
  ): Promise<ApiResult<SearchResult[]>> => {
    const effectiveLimit = limit ?? 10;
    const effectiveSearchMode = searchMode ?? 'hybrid';
    const raw = await apiCall<unknown>('hybrid_search', {
      args: {
        query,
        limit: effectiveLimit,
        search_mode: effectiveSearchMode,
      },
    });
    const primary = unwrapSearchResults(raw, 'Hybrid search failed');
    if (!primary.ok || primary.data.length > 0) {
      return primary;
    }

    // Fallback: use the unified search command path if the direct hybrid command
    // unexpectedly yields an empty payload shape.
    const fallbackRaw = await apiCall<unknown>('search_documents', {
      options: {
        query,
        limit: effectiveLimit,
        searchMode: effectiveSearchMode,
      },
    });
    return unwrapSearchResults(fallbackRaw, 'Hybrid search failed');
  },

  /**
   * Performs pure semantic vector similarity search.
   * Uses embeddings to find semantically similar documents without keyword matching.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (default: 10)
   * @returns Array of search results ranked by semantic similarity
   */
  searchSemantic: async (query: string, limit?: number): Promise<ApiResult<SearchResult[]>> => {
    const request: Wire.SearchRequestDto = { query, limit: limit ?? 10, threshold: null, mode: { type: 'vector' } };
    const raw = await apiCall<unknown>('semantic_search', { request });
    const primary = unwrapSearchResults(raw, 'Semantic search failed');
    if (!primary.ok || primary.data.length > 0) {
      return primary;
    }

    const fallbackRaw = await apiCall<unknown>('search_documents', {
      options: {
        query,
        limit: limit || 10,
        searchMode: 'semantic',
      },
    });
    return unwrapSearchResults(fallbackRaw, 'Semantic search failed');
  },

  /**
   * Finds documents similar to a given document by chunk ID.
   * Uses vector similarity to find related content.
   *
   * @param chunkId - ID of the chunk to find similar documents for
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of similar documents ranked by similarity score
   */
  findSimilar: async (chunkId: string, limit?: number): Promise<ApiResult<SearchResult[]>> => {
    const result = await apiCall<Wire.SearchResultDto[]>('find_similar', { chunkId, limit });
    return unwrapSearchResults(result, 'Similar document search failed');
  },

  /**
   * Finds documents whose content is closest to a given document.
   *
   * Document-shaped where `findSimilar` is chunk-shaped: the backend picks a
   * representative chunk, searches, and collapses the hits to one row per
   * document, so the caller never has to know a document's chunk ids.
   *
   * @param documentId - The document to find neighbours for
   * @param limit - Maximum number of documents to return (default 6)
   */
  findSimilarDocuments: async (
    documentId: string,
    limit?: number,
  ): Promise<ApiResult<SimilarDocumentDto[]>> =>
    apiCall<Wire.SimilarDocumentDto[]>('find_similar_documents', { documentId, limit }),



  /**
   * Starts indexing a folder and all its contents.
   * Extracts text, generates embeddings, and stores in database.
   * Supports recursive directory traversal.
   *
   * @param path - Absolute path to folder to index
   * @param recursive - Whether to index subdirectories (default: true)
   * @param spaceId - Optional space to assign indexed documents to
   * @returns Void on success
   */
  startIndexing: async (
    path: string,
    recursive?: boolean,
    spaceId?: string
  ): Promise<ApiResult<void>> =>
    apiCall<void>('index_directory', {
      path,
      recursive: recursive ?? true,

      spaceId,
    }),

  /**
   * Indexes a single file by extracting content and generating embeddings.
   * Supports markdown, text, PDF, DOCX, and other document formats.
   *
   * @param path - Absolute path to file to index
   * @param spaceId - Optional space to assign the indexed document to
   * @returns IndexFileResponse with status information
   */
  indexFile: async (path: string, spaceId?: string): Promise<ApiResult<IndexFileResponse>> =>
    apiCall<Wire.IndexFileResponseDto>('index_file', { path, spaceId }),

  /**
   * Reindexes an existing file with fresh content and embeddings.
   * Useful when file content has changed or to update metadata.
   *
   * @param path - Absolute path to file to reindex
   * @returns Void on success
   */
  reindexFile: async (path: string): Promise<ApiResult<void>> => apiCall<void>('reindex_file', { path }),

  /**
   * Removes a file from the search index and database.
   * Deletes document record, embeddings, and all associated metadata.
   *
   * @param path - Absolute path to file to remove
   * @returns Void on success
   */
  removeIndexedFile: async (path: string): Promise<ApiResult<void>> => apiCall<void>('remove_indexed_file', { path }),

  /**
   * Deletes a document by ID from the search index and database.
   * Removes document record, all chunks, and vector embeddings.
   * This operation is irreversible.
   *
   * @param documentId - Unique identifier of the document to delete
   * @returns Void on success
   */
  deleteDocument: async (documentId: string): Promise<ApiResult<void>> => apiCall<void>('delete_document', { documentId }),

  /**
   * Renames a document's display name in the database.
   * Only updates the metadata - does NOT modify file content or regenerate embeddings.
   * This follows the user's requirement that modifying content would be too complicated.
   *
   * @param documentId - Unique identifier of the document to rename
   * @param newName - New display name (1-255 chars, no path separators)
   * @returns Response with confirmation and new name
   *
   * @example
   * const result = await VaultAPI.renameDocument('doc-123', 'New Name.pdf');
   * if (result.ok) {
   *   console.log(result.data.message); // "Document renamed from '...' to '...'"
   * }
   */
  renameDocument: async (
    documentId: string,
    newName: string
  ): Promise<ApiResult<Wire.RenameDocumentResponseDto>> => apiCall<Wire.RenameDocumentResponseDto>(
      'rename_document',
      { documentId, newName }
    ),

  /**
   * Retrieves current indexing progress for active indexing operations.
   * Includes files processed, total files, current file, and completion percentage.
   *
   * @returns IndexProgress object with current state
   */
  getIndexProgress: async (): Promise<ApiResult<IndexingSnapshot>> => {
    const result = await apiCall<Wire.IndexProgress>('get_index_progress');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexProgress(result.data) };
  },

  /**
   * Cancels any currently running indexing operation.
   * Gracefully stops processing and preserves already-indexed files.
   *
   * @returns Void on success
   */
  cancelIndexing: async (): Promise<ApiResult<void>> => apiCall<void>('cancel_indexing'),

  /** Pauses the running index. Both the directory run and the watcher queue stop. */
  pauseIndexing: async (): Promise<ApiResult<void>> => apiCall<void>('pause_indexing'),

  /** Resumes a paused index. */
  resumeIndexing: async (): Promise<ApiResult<void>> => apiCall<void>('resume_indexing'),

  /**
   * Drops one path from the run's failure list.
   *
   * The list lives in the indexing engine's state, which is what every view
   * reads through `getIndexProgress` — dismissing a row locally would let it
   * reappear the moment another surface reads the snapshot. Unknown paths are
   * a no-op on the backend.
   */
  clearIndexingFailure: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('clear_indexing_failure', { path }),

  /**
   * Thin `{ active, progress }` view of the index.
   * Prefer `getIndexProgress`, which carries the counts, the status word, the
   * pause flag and the failure list.
   */
  getIndexingStatus: async (): Promise<ApiResult<{ active: boolean; progress: number }>> =>
    apiCall<Wire.IndexingStatus>('get_indexing_status'),

  /** Absolute paths of indexed files, newest first. */
  listIndexedFiles: async (limit?: number): Promise<ApiResult<string[]>> =>
    apiCall<string[]>('list_indexed_files', { limit }),


  /**
   * Writes a database backup into the app's backups folder and returns its path.
   * The destination is fixed by the backend (CWE-22 confinement); there is no
   * folder argument to pass.
   */
  createBackup: async (): Promise<ApiResult<CreateBackupResult>> =>
    apiCall<Wire.CreateBackupResultDto>('plugin_create_backup', { request: { backupPath: null } }),

  /** Backups on disk, newest first. */
  listBackups: async (): Promise<ApiResult<BackupInfo[]>> => {
    const result = await apiCall<Wire.ListBackupsResultDto>('plugin_list_backups');
    if (!result.ok) return result;
    return { ok: true, data: result.data.backups ?? [] };
  },

  /**
   * Replaces the live database with a backup. The backend closes the connection
   * pool and does not reopen it — the app must be quit and reopened afterwards.
   */
  restoreBackup: async (backupPath: string): Promise<ApiResult<RestoreBackupResult>> =>
    apiCall<Wire.RestoreBackupResultDto>('plugin_restore_backup', { request: { backupPath } }),

  /** Exports conversations and journal pages as Markdown into the app's exports folder. */
  exportMarkdown: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_markdown', { request: { outputDir: null } }),

  /** Exports conversations and journal pages as one JSON file in the app's exports folder. */
  exportJson: async (): Promise<ApiResult<ExportSummary>> =>
    apiCall<Wire.ExportResultDto>('plugin_export_json', { request: { outputPath: null, pretty: true } }),

  /* ---------------------------------------------------------------- *
   * Off-device backup (encrypted archive).
   *
   * Every path in this group is chosen by a native picker on the Rust side —
   * the webview neither sends nor receives a folder to write to. The generic
   * on each `apiCall` is the local DTO type in `types/api/backup.ts` rather
   * than a `Wire.*` type, because these commands are added to
   * `lib/bindings.ts` by `npm run bindings:generate` on the Rust side.
   * ---------------------------------------------------------------- */

  /** Current archive configuration, last run, and the archives on disk. */
  getArchiveStatus: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_get_archive_status') as Promise<ApiResult<ArchiveStatus>>,

  /**
   * Generates a master key and a 24-word recovery code, held in memory only.
   * Nothing is written until `confirmArchiveSetup` succeeds; calling again
   * throws the previous pending setup away.
   */
  beginArchiveSetup: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_begin_archive_setup'),

  /**
   * Proves the user wrote the recovery code down, then persists the envelope.
   * `passphrase` is optional and must be at least 8 characters when present.
   */
  confirmArchiveSetup: async (
    confirmations: WordConfirmation[],
    passphrase: string | null,
  ): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_confirm_archive_setup', {
      request: { confirmations, passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /** Opens the Rust-side folder picker. Cancelling returns the status unchanged. */
  chooseArchiveDestination: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_choose_archive_destination') as Promise<ApiResult<ArchiveStatus>>,

  /** How many archives to keep in the destination folder (1–50). */
  setArchiveKeepCount: async (keepCount: number): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_keep_count', {
      request: { keepCount },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /** Sets, changes, or (with `null`) removes the passphrase slot. */
  setArchivePassphrase: async (passphrase: string | null): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_set_archive_passphrase', {
      request: { passphrase },
    }) as Promise<ApiResult<ArchiveStatus>>,

  /**
   * Issues a new recovery code. Confirm it with `confirmArchiveSetup`; the
   * passphrase slot is untouched, so pass `null` for the passphrase there.
   */
  rotateRecoveryCode: async (): Promise<ApiResult<ArchiveSetup>> =>
    apiCall<Wire.ArchiveSetupDto>('plugin_rotate_recovery_code'),

  /** Stops writing archives. The key and the envelope stay, so it can be turned back on. */
  disableArchive: async (): Promise<ApiResult<ArchiveStatus>> =>
    apiCall<Wire.ArchiveStatusDto>('plugin_disable_archive') as Promise<ApiResult<ArchiveStatus>>,

  /** Writes one archive now, outside the schedule. */
  createArchiveNow: async (): Promise<ApiResult<ArchiveRun>> =>
    apiCall<Wire.ArchiveRunDto>('plugin_create_archive_now'),

  /**
   * Restores from an archive the user picks in the Rust-side file dialog.
   * Pass `null` first: the keyring usually holds the key already. A
   * `needs_secret` outcome means asking for the passphrase or the recovery
   * code and calling again with it.
   */
  restoreArchive: async (secret: string | null): Promise<ApiResult<RestoreArchiveResult>> =>
    apiCall<Wire.RestoreArchiveResultDto>('plugin_restore_archive', {
      request: { secret },
    }) as Promise<ApiResult<RestoreArchiveResult>>,

  /**
   * Gets comprehensive statistics about indexed content.
   * Includes total documents, total size, file type breakdown, and more.
   *
   * @returns IndexingStats object with counts and metrics
   */
  getIndexingStats: async (): Promise<ApiResult<IndexingStats>> => {
    const result = await apiCall<Wire.IndexingStatsDto>('get_indexing_stats');
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeIndexingStats(result.data) };
  },

  /**
   * Lists all folders that have been indexed.
   * Returns folder paths with document counts and last indexed timestamps.
   *
   * @returns Array of IndexedFolder objects
   */
  getIndexedFolders: async (): Promise<ApiResult<IndexedFolder[]>> => apiCall<Wire.IndexedFolder[]>('get_indexed_folders'),

  /**
   * Retrieves recent indexing activity history.
   * Shows files indexed, errors, and timestamps for debugging and monitoring.
   *
   * @param limit - Maximum number of activity records to return
   * @returns Array of IndexingActivity objects ordered by recency
   */
  getIndexingActivities: async (limit: number): Promise<ApiResult<IndexingActivity[]>> => apiCall<Wire.IndexingActivity[]>('get_indexing_activities', { limit }),


  /**
   * Opens a file in the system's default application.
   * Uses platform-specific "open" command (e.g., xdg-open, open, start).
   *
   * @param path - Absolute path to file to open
   * @returns OpenFileResponseDto with success status and optional error
   */
  openFile: async (path: string): Promise<ApiResult<OpenFileResponseDto>> => apiCall<OpenFileResponseDto>('open_file', { path }),

  /**
   * Opens a file by its internal document ID.
   * Resolves the ID to a file path and either opens in default application
   * or returns internal rendering info for web articles.
   *
   * @param fileId - Internal document identifier
   * @returns OpenFileResponseDto indicating action taken
   */
  openFileById: async (fileId: string): Promise<ApiResult<OpenFileResponseDto>> => apiCall<OpenFileResponseDto>('open_file_by_id', { fileId }),

  /**
   * Opens a native file picker dialog for the user to select a single file.
   * Uses Tauri's dialog plugin to provide a platform-native file selection experience.
   *
   * @returns Selected file path or null if cancelled
   *
   * @example
   * const result = await VaultAPI.selectFile();
   * if (result) {
   *   console.log('Selected file:', result);
   *   await VaultAPI.indexFile(result);
   * }
   */
  selectFile: async (): Promise<string | null> => {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: 'Select File',
      });
      return selected;
    } catch (error) {
      console.error('File selection error:', error);
      return null;
    }
  },

  /**
   * Opens a native file picker dialog for the user to select multiple files.
   * Uses Tauri's dialog plugin to provide a platform-native file selection experience.
   *
   * @returns Array of selected file paths or empty array if cancelled
   *
   * @example
   * const result = await VaultAPI.selectMultipleFiles();
   * if (result.length > 0) {
   *   console.log('Selected files:', result);
   *   for (const filePath of result) {
   *     await VaultAPI.indexFile(filePath);
   *   }
   * }
   */
  selectMultipleFiles: async (): Promise<string[]> => {
    try {
      const selected = await open({
        directory: false,
        multiple: true,
        title: 'Select Files',
      });
      // open() returns string[] when multiple=true, string when multiple=false
      if (Array.isArray(selected)) {
        return selected;
      }
      // If null or undefined, return empty array
      return [];
    } catch (error) {
      console.error('Multiple file selection error:', error);
      return [];
    }
  },

  /**
   * Opens a native folder picker dialog for the user to select a folder.
   * Uses Tauri's dialog plugin to provide a platform-native folder selection experience.
   * Commonly used for adding folders to index or selecting output directories.
   *
   * @returns Selected folder path or null if cancelled
   *
   * @example
   * const result = await VaultAPI.selectFolder();
   * if (result) {
   *   console.log('Selected folder:', result);
   *   await VaultAPI.startIndexing(result, true);
   * }
   */
  selectFolder: async (): Promise<string | null> => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder',
      });
      return selected;
    } catch (error) {
      console.error('Folder selection error:', error);
      return null;
    }
  },

  /**
   * Retrieves the file system path for a document by its ID.
   * Useful for displaying paths or performing file operations.
   *
   * @param fileId - Internal document identifier
   * @returns Absolute file path as string
   */
  getFilePathById: async (fileId: string): Promise<ApiResult<string>> => apiCall<string>('get_file_path_by_id', { fileId }),

  /**
   * Gets recently accessed documents ordered by last access time.
   * Useful for building "Recent Files" UI features.
   *
   * @param limit - Maximum number of recent documents to return
   * @returns Array of RecentDocument objects with paths and timestamps
   */
  getRecentDocuments: async (limit: number): Promise<ApiResult<RecentDocument[]>> => apiCall<Wire.RecentDocument[]>('get_recent_documents', { limit }),

  /**
   * Lists ALL documents from the documents table.
   * This is the correct command for file browser UIs that need to show the complete library.
   *
   * Unlike getRecentDocuments which only returns tracked documents, this returns ALL documents
   * including newly imported web archives, PDFs, etc. that haven't been accessed yet.
   *
   * @param limit - Maximum number of documents to return (capped at 10000)
   * @returns Array of all documents with metadata
   */
  listAllDocuments: async (limit: number): Promise<ApiResult<DocumentMetadata[]>> => apiCall<Wire.DocumentMetadataDto[]>('list_all_documents', { limit }),

  /**
   * Retrieves document metadata by ID.
   *
   * Used to get full document information (especially file path) when you only have
   * the document ID, such as from recent documents or search results.
   *
   * @param documentId - Unique document identifier (UUID)
   * @returns Document metadata including file path for opening
   *
   * @example
   * ```typescript
   * // Get document to open from recent list
   * const result = await VaultAPI.getDocument(docId);
   * if (result.ok && result.data.filePath) {
   *   const openResult = await VaultAPI.openFile(result.data.filePath);
   *   if (!openResult.ok) {
   *     console.error('Failed to open:', openResult.error);
   *   }
   * }
   * ```
   */
  getDocument: async (documentId: string): Promise<ApiResult<DocumentMetadata>> =>
    apiCall<DocumentMetadata>('get_document', { documentId }),

  /**
   * Reads the raw text content of a file from disk.
   * Returns UTF-8 decoded content for text-based formats.
   *
   * @param path - Absolute path to file to read
   * @returns File content as string
   */
  readFileContent: async (path: string): Promise<ApiResult<string>> => apiCall<string>('read_file_content', { path }),

  /**
   * Reads the raw bytes of a file from disk.
   *
   * @param path - Absolute path to file to read
   * @returns File content as bytes
   */
  readFileBytes: async (path: string): Promise<ApiResult<Uint8Array>> => {
    const result = await apiCall<number[]>('read_file_bytes', { path });
    if (!result.ok) return result;
    return { ok: true, data: Uint8Array.from(result.data) };
  },

  /**
   * Retrieves file system metadata for a file.
   * Includes size, creation time, modification time, and permissions.
   *
   * @param path - Absolute path to file
   * @returns Metadata object with file system information
   */
  getFileMetadata: async (path: string): Promise<ApiResult<FileMetadata>> => apiCall<Wire.FileMetadataDto>('get_file_metadata', { path }),

  /**
   * Opens the file's parent folder in the system file manager.
   * Highlights the file if the platform supports it (e.g., Finder, Explorer).
   *
   * @param path - Absolute path to file
   * @returns Void on success
   */
  showInFolder: async (path: string): Promise<ApiResult<void>> => apiCall<void>('show_in_folder', { path }),


  /**
   * Adds a folder to the file watcher.
   * Enables automatic reindexing when files in this folder change.
   *
   * @param path - Absolute path to folder to watch
   * @returns Void on success
   */
  addWatchFolder: async (path: string): Promise<ApiResult<void>> => apiCall<void>('add_watch_folder', { path }),

  /**
   * Removes a folder from the file watcher.
   * Stops monitoring for file changes in this folder.
   *
   * @param path - Absolute path to folder to stop watching
   * @returns Void on success
   */
  removeWatchFolder: async (path: string): Promise<ApiResult<void>> => apiCall<void>('remove_watch_folder', { path }),


  /**
   * Fetches preview metadata for a web URL without ingesting it.
   * Useful for validating and showing a preview before import.
   *
   * @param url - Web URL to preview (must be valid HTTP/HTTPS)
   * @returns UrlPreview metadata
   */
  fetchUrlPreview: async (url: string): Promise<ApiResult<UrlPreview>> =>
    apiCall<Wire.UrlPreview>('fetch_url_preview', { url }),

  /**
   * Extracts article content (reader mode) from a URL.
   * Returns clean text/HTML without ingesting.
   *
   * @param url - Web URL to extract
   * @returns CleanArticle content
   */
  extractArticle: async (url: string): Promise<ApiResult<CleanArticle>> =>
    apiCall<Wire.CleanArticle>('extract_article', { url }),

  /**
   * Ingests content from a web URL and adds it to the index.
   * Fetches page, extracts text, generates embeddings, and stores as document.
   *
   * @param url - Web URL to ingest (must be valid HTTP/HTTPS)
   * @returns WebIngestResponse with document ID and metadata
   */
  ingestWebUrl: async (
    url: string,
    options?: { spaceId?: string; conversationId?: string }
  ): Promise<ApiResult<WebIngestResponse>> =>
    apiCall<Wire.WebIngestResponse>('ingest_web_url', {
      url,

      spaceId: options?.spaceId,

      conversationId: options?.conversationId,
    }),

  // `ingestWebUrlsBatch` was removed: `ingest_web_urls_batch` is not a
  // registered Tauri command and its `BatchIngestSummary` type was never
  // defined, so every call would have failed at runtime. Nothing referenced
  // it. Re-add it together with the backend command if batch ingest is built.


  /**
   * Retrieves the daily note for today's date.
   * Creates a new daily note if one doesn't exist for today.
   *
   * @returns Daily note object with content and metadata
   */
  getTodayNote: async (): Promise<ApiResult<DailyNote>> => apiCall<Wire.DailyNoteCompatDto>('get_today_note'),

  /**
   * Quickly captures text to today's daily note.
   * Appends content to existing note or creates new note if needed.
   * Ideal for inbox-style quick capture workflows.
   *
   * @param content - Text content to append to today's note
   * @returns Which page the capture landed on, so the UI can name it
   */
  quickCapture: async (content: string): Promise<ApiResult<QuickCaptureResultDto>> => apiCall<Wire.QuickCaptureResultDto>('quick_capture', { content }),

  /**
   * Retrieves all daily notes within a date range.
   * Useful for calendar views and date-based navigation.
   *
   * @param startDate - Start date in ISO format (YYYY-MM-DD)
   * @param endDate - End date in ISO format (YYYY-MM-DD)
   * @returns Array of daily note objects within the range
   */
  getDailyNotesRange: async (startDate: string, endDate: string): Promise<ApiResult<DailyNote[]>> =>
    apiCall<Wire.DailyNoteCompatDto[]>('get_daily_notes_range', { request: { startDate, endDate } }),

  /**
   * Gets the daily note immediately before the specified date.
   * Returns null if no earlier note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Previous daily note or null
   */
  getPreviousDailyNote: async (currentDate: string): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_previous_daily_note', { request: { currentDate } }),

  /**
   * Gets the daily note immediately after the specified date.
   * Returns null if no later note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Next daily note or null
   */
  getNextDailyNote: async (currentDate: string): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_next_daily_note', { request: { currentDate } }),

  /**
   * Updates the content of an existing daily note.
   * Completely replaces the note's content with new text.
   *
   * @param noteId - Internal identifier of the daily note
   * @param content - New content to save
   * @returns Void on success
   */
  updateDailyNoteContent: async (noteId: string, content: string): Promise<ApiResult<void>> =>
    apiCall<void>('update_daily_note_content', { request: { noteId, content } }),

  /**
   * Lists persisted notes in the Daily Notes workspace.
   *
   * With a `journalId`, only the pages that journal owns — which is what a
   * journal's sidebar wants. Without one, every page across every journal,
   * for the cross-journal surfaces (reference inbox, weekly synthesis).
   */
  listWorkspaceNotes: async (journalId?: string): Promise<ApiResult<ListWorkspaceNotesResponse>> =>
    apiCall<Wire.ListWorkspaceNotesResponseDto>('list_workspace_notes', {
      request: { journalId: journalId ?? null },
    }),

  /**
   * Whether reranking is switched on, and whether its model is on disk.
   *
   * The two are independent: the setting can be on with nothing installed, in
   * which case search silently returns its unreranked shortlist. `active` is
   * the only field that says what will actually happen.
   */
  getRerankerStatus: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('reranker_status'),

  /**
   * Fetches the reranker model. Does not switch reranking on — acquiring the
   * model and choosing to use it stay separate decisions.
   */
  downloadReranker: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('download_reranker'),

  /**
   * Creates a new persisted workspace note, owned by `journalId` when given.
   * An unowned page is unfiled and appears in no journal's page list.
   */
  createWorkspaceNote: async (
    title?: string,
    journalId?: string,
  ): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('create_workspace_note', {
      request: { title, journalId: journalId ?? null },
    }),

  /**
   * Persists a workspace note update.
   */
  updateWorkspaceNote: async (note: WorkspaceNote): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('update_workspace_note', { note }),

  /**
   * Deletes a persisted workspace note by id.
   */
  deleteWorkspaceNote: async (noteId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_workspace_note', { request: { noteId } }),


  /**
   * Retrieves all application settings.
   * Returns complete settings object with all categories.
   *
   * @returns Settings object with all configuration values
   */
  getSettings: async (): Promise<ApiResult<AppSettings>> => apiCall<AppSettings>('get_settings'),

  /**
   * Gets settings for a specific category.
   * Categories include 'appearance', 'search', 'indexing', etc.
   *
   * @param category - Settings category name
   * @returns Settings object for the specified category
   */
  getSettingsCategory: async (category: string): Promise<ApiResult<Record<string, unknown>>> => apiCall<Record<string, unknown>>('get_settings_category', { category }),

  /**
   * Updates one or more settings.
   * Merges provided settings with existing configuration.
   *
   * @param settings - Partial settings object with values to update
   * @returns Updated settings object
   */
  updateSettings: async (settings: Wire.UpdateSettingsRequest): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('update_settings', { settings }),

  /**
   * Resets all settings to default values.
   * Clears user customizations and restores factory defaults.
   *
   * @returns Updated settings object
   */
  resetSettings: async (): Promise<ApiResult<AppSettings>> => apiCall<AppSettings>('reset_settings'),

  /**
   * Exports all settings as JSON string.
   * Useful for backup or transferring settings between devices.
   *
   * @returns JSON string containing all settings
   */
  exportSettings: async (): Promise<ApiResult<string>> => apiCall<string>('export_settings'),

  /**
   * Imports settings from a JSON string.
   * Replaces current settings with imported configuration.
   *
   * @param settings - JSON string containing settings to import
   * @param merge - If true, merge with existing settings instead of replacing
   * @returns Updated settings object
   */
  importSettings: async (
    settings: string,
    merge?: boolean
  ): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('import_settings', {
      request: {
        settings,
        merge,
      },
    }),

  /**
   * Detects the system theme preference.
   * Returns 'light' or 'dark' based on OS-level settings.
   *
   * @returns System theme as string ('light' or 'dark')
   */
  getSystemTheme: async (): Promise<ApiResult<string>> => apiCall<string>('get_system_theme'),

  /**
   * Validates that a folder path exists and is accessible.
   * Checks permissions and path validity before indexing operations.
   *
   * @param path - Absolute folder path to validate
   * @returns True if path is valid and accessible, false otherwise
   */
  validateFolderPath: async (path: string): Promise<ApiResult<boolean>> => apiCall<boolean>('validate_folder_path', { path }),

  /** Checks llama.cpp model discovery and a small chat completion. */
  testLlamaCppConnection: async (
    request: Wire.LlamaCppSettingsDto
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_llama_cpp_connection', { request }),

  /** Lists models through the native Ollama API. */
  testOllamaConnection: async (
    request: TestOllamaConnectionRequest
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_ollama_connection', { request }),

  /**
   * Tests a custom tool endpoint with a sample query.
   * Useful for validating endpoint/query-param wiring before saving.
   */
  testCustomTool: async (
    request: TestCustomToolRequest
  ): Promise<ApiResult<TestCustomToolResponse>> =>
    apiCall<Wire.TestCustomToolResponse>('test_custom_tool', { request }),


  /**
   * Retrieves all tags in the system.
   * Returns tags with document counts and metadata.
   *
   * @returns Array of all tags with full metadata
   *
   * @example
   * const result = await VaultAPI.listAllTags();
   * if (result.ok) {
   *   console.log(`Found ${result.data.tags.length} tags`);
   * }
   */
  listAllTags: async (): Promise<ApiResult<ListTagsResponse>> => {
    const result = await apiCall<Wire.TagWithCountDto[]>('get_all_tags_with_counts');
    if (!result.ok) return result;
    return { ok: true, data: { tags: result.data.map(tag => ({ ...tag, description: null })) } };
  },

  /**
   * Removes a tag assignment from a document.
   * Tag itself is not deleted, only the association.
   * Idempotent operation.
   *
   * @param request - Remove tag request with document and tag IDs
   * @returns Void on success
   *
   * @example
   * const result = await VaultAPI.removeTagFromDocument({
   *   documentId: 'doc_123',
   *   tagId: 'tag_456'
   * });
   */
  removeTagFromDocument: async (request: RemoveTagFromDocumentRequest): Promise<ApiResult<void>> =>
    apiCall<void>('remove_tag_from_document', { request: {
      document_id: request.documentId,
      tag_id: request.tagId,
    } }),

  /**
   * Retrieves all tags applied to a specific document.
   *
   * @param request - Request with document ID
   * @returns Document tags response with tag list
   *
   * @example
   * const result = await VaultAPI.getDocumentTags({ documentId: 'doc_123' });
   * if (result.ok) {
   *   console.log(`Document has ${result.data.tags.length} tags`);
   * }
   */
  getDocumentTags: async (request: GetDocumentTagsRequest): Promise<ApiResult<DocumentTagsResponse>> => {
    const result = await apiCall<Wire.TagDto[]>('get_document_tags', { documentId: request.documentId });
    if (!result.ok) return result;
    return { ok: true, data: { documentId: request.documentId, tags: result.data } };
  },

  /**
   * Retrieves all tags with document counts.
   * Useful for tag clouds and filter UIs that show tag popularity.
   * Tags are returned sorted by document count (descending).
   *
   * @returns Array of tags with document counts
   *
   * @example
   * const result = await VaultAPI.getAllTagsWithCounts();
   * if (result.ok) {
   *   result.data.forEach(tag => {
   *     console.log(`${tag.name}: ${tag.document_count} documents`);
   *   });
   * } else {
   *   console.error('Failed to load tags:', result.error);
   * }
   */
  getAllTagsWithCounts: async (): Promise<ApiResult<Wire.TagWithCountDto[]>> =>
    apiCall<Wire.TagWithCountDto[]>('get_all_tags_with_counts'),

  /**
   * Applies multiple tags to a document by name.
   * Creates tags if they don't exist. Returns updated tag list.
   * This is a convenience method that handles both tag creation and assignment.
   *
   * @param documentId - Document to tag
   * @param tagNames - Array of tag names to apply (case-insensitive)
   * @returns Array of applied tags with full metadata
   *
   * @example
   * const result = await VaultAPI.applyTags('doc_123', ['research', 'ai', 'important']);
   * if (result.ok) {
   *   console.log(`Applied ${result.data.length} tags`);
   *   result.data.forEach(tag => console.log(`- ${tag.name}`));
   * } else {
   *   console.error('Failed to apply tags:', result.error);
   * }
   */
  applyTags: async (
    documentId: string,
    tagNames: string[]
  ): Promise<ApiResult<Tag[]>> =>
    apiCall<Wire.TagDto[]>('apply_tags', {
      request: {
        document_id: documentId,
        tag_names: tagNames
      }
    }),

  /**
   * Generates tags for a document using LLM analysis.
   * Backend fetches document content and analyzes it.
   * This is a convenience wrapper for document-based tag generation.
   *
   * @param documentId - Document to generate tags for
   * @returns Array of suggested tag names
   *
   * @example
   * const result = await VaultAPI.generateTagsForDocument('doc_123');
   * if (result.ok) {
   *   console.log('Suggested tags:', result.data);
   *   await VaultAPI.applyTags('doc_123', result.data);
   * }
   */
  generateTagsForDocument: async (documentId: string): Promise<ApiResult<string[]>> =>
    apiCall<string[]>('generate_tags_for_document', {
      request: {
        document_id: documentId,
        max_tags: 5
      }
    }),


  /**
   * Asks a question and retrieves an AI-generated answer based on indexed content.
   * Uses retrieval-augmented generation (RAG) to find relevant documents
   * and generate contextual answers.
   *
   * @param question - Question to answer
   * @param contextLimit - Optional limit for number of context documents (default: 5)
   * @returns Answer object with generated text and source references
   */
  askQuestion: async (question: string, contextLimit?: number): Promise<ApiResult<QAResponse>> => {
    const request = { question, context_limit: contextLimit };
    return apiCall<QAResponse>('ask_question', { request });
  },

  /**
   * Asks a question with streaming response.
   * Returns answer incrementally as it's generated for better UX.
   * Note: Currently falls back to non-streaming implementation.
   *
   * @param question - Question to answer
   * @param contextLimit - Optional limit for number of context documents (default: 5)
   * @returns Answer object with generated text (streaming not yet implemented)
   */
  askQuestionStream: async (question: string, contextLimit?: number): Promise<ApiResult<QAResponse>> => {
    const request = { question, context_limit: contextLimit };
    return apiCall<QAResponse>('ask_question_stream', { request });
  },

  /**
   * Checks if the LLM (Large Language Model) service is healthy and accessible.
   * Verifies API key, endpoint connectivity, and model availability.
   *
   * @returns Health status object with connectivity and model info
   */
  checkLLMHealth: async (): Promise<ApiResult<LLMHealthStatus>> => apiCall<LLMHealthStatus>('check_llm_health'),

  /**
   * Three corpus-derived questions for the Chat empty state, drawn from the
   * documents `spaceId` can see. A null space means General.
   * Returns an empty `starters` array when no model could produce them —
   * the empty state renders no questions rather than inventing any.
   */
  generateChatStarters: async (spaceId?: string | null): Promise<ApiResult<ChatStarters>> =>
    apiCall<ChatStarters>('generate_chat_starters', { spaceId: spaceId ?? null }),

  /**
   * Saves an excerpt from a document as a reference.
   */
  createPassageReference: async (
    request: CreatePassageReferenceRequest
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall<PassageReferenceDto>('create_passage_reference', { request }),

  /** Saved passage references, newest first. */
  listPassageReferences: async (
    limit?: number
  ): Promise<ApiResult<PassageReferenceDto[]>> =>
    apiCall<PassageReferenceDto[]>('list_passage_references', { limit }),

  /** Updates a passage reference's title and note. `null` clears a field. */
  updatePassageReference: async (
    request: UpdatePassageReferenceRequest
  ): Promise<ApiResult<PassageReferenceDto>> =>
    apiCall<PassageReferenceDto>('update_passage_reference', { request }),

  /** Deletes a saved passage reference. */
  deletePassageReference: async (id: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_passage_reference', { id }),

  /** Builds a comparison table across documents. */
  listStudyDecks: (): Promise<ApiResult<Wire.StudyDeckSummaryDto[]>> => apiCall('list_study_decks'),
  getStudyDeck: (id: string): Promise<ApiResult<Wire.StudyDeckDto>> => apiCall('get_study_deck', { id }),
  generateStudyDeck: (request: Wire.GenerateStudyDeckRequestDto): Promise<ApiResult<Wire.StudyDeckDto>> => apiCall('generate_study_deck', { request }),
  generateConversationStudyDeck: (request: Wire.GenerateConversationStudyDeckRequestDto): Promise<ApiResult<Wire.StudyDeckDto>> => apiCall('generate_conversation_study_deck', { request }),
  reviewStudyCard: (request: Wire.ReviewStudyCardRequestDto): Promise<ApiResult<Wire.StudyCardDto>> => apiCall('review_study_card', { request }),
  updateStudyCard: (request: Wire.UpdateStudyCardRequestDto): Promise<ApiResult<void>> => apiCall('update_study_card', { request }),
  deleteStudyDeck: (id: string): Promise<ApiResult<void>> => apiCall('delete_study_deck', { id }),

  compareDocuments: async (
    request: CompareDocumentsRequest
  ): Promise<ApiResult<CompareTableDto>> =>
    apiCall<CompareTableDto>('compare_documents', { request }),


  /**
   * Marks a document as favorite.
   * Adds document to favorites list for quick access.
   *
   * @param documentId - Internal document identifier
   * @returns Void on success
   */
  addFavorite: async (documentId: string): Promise<ApiResult<void>> => apiCall<void>('add_favorite', { documentId }),

  /**
   * Removes a document from favorites.
   * Unfavorites the document but doesn't delete it.
   *
   * @param documentId - Internal document identifier
   * @returns Void on success
   */
  removeFavorite: async (documentId: string): Promise<ApiResult<void>> => apiCall<void>('remove_favorite', { documentId }),

  /**
   * Retrieves all favorited documents.
   * Returns documents in the order they were favorited.
   *
   * @returns Array of favorite document objects
   */
  getFavorites: async (): Promise<ApiResult<FavoriteDocument[]>> => apiCall<Wire.FavoriteDocument[]>('get_favorites'),

  /**
   * Checks if a document is marked as favorite.
   *
   * @param documentId - Internal document identifier
   * @returns True if document is favorited, false otherwise
   */
  isFavorite: async (documentId: string): Promise<ApiResult<boolean>> => apiCall<boolean>('is_favorite', { documentId }),




  /**
   * Clears the search results cache.
   * Forces fresh results on next search instead of using cached data.
   *
   * @returns Void on success
   */
  clearSearchCache: async (): Promise<ApiResult<void>> => apiCall<void>('clear_search_cache'),

  /**
   * Clears all caches (search, embeddings, etc.).
   * Comprehensive cache clear for troubleshooting or memory management.
   *
   * @returns Void on success
   */
  clearCache: async (): Promise<ApiResult<void>> => apiCall<void>('clear_cache'),

  /**
   * Retrieves cache statistics.
   * Includes hit rate, size, and entry count for all caches.
   *
   * @returns Cache statistics object
   */
  getCacheStats: async (): Promise<ApiResult<Wire.SearchCacheStats>> => apiCall<Wire.SearchCacheStats>('get_cache_stats'),

  /**
   * Gets detailed cache performance metrics.
   * Returns metrics for monitoring and optimization.
   *
   * @returns Cache metrics object with detailed statistics
   */
  getCacheMetrics: async (): Promise<ApiResult<CacheMetrics>> => apiCall<Wire.CacheMetrics>('get_cache_metrics'),


  /**
   * Performs a health check on the backend system (DDD architecture).
   * Verifies database connectivity, model availability, and system status.
   *
   * @returns Health status object with component statuses
   * Uses the current domain command.
   */
  healthCheck: async (): Promise<ApiResult<HealthStatus>> => apiCall<Wire.HealthStatus>('health_check'),

  /**
   * Retrieves system statistics (DDD architecture).
   * Includes total documents, chunks, tags, and storage size.
   *
   * @returns System statistics object
   * Uses the current domain command.
   */
  getSystemStats: async (): Promise<ApiResult<SystemStats>> => apiCall<Wire.SystemStats>('get_system_stats'),

  /**
   * Gets the application version from Cargo.toml.
   * Returns semantic version string for display and update checking.
   *
   * @returns Version string (e.g., "0.1.0")
   */
  getVersion: async (): Promise<ApiResult<string>> => apiCall<string>('get_version'),

  /**
   * Gets detailed version information including build metadata.
   * Returns comprehensive version info for troubleshooting and support.
   *
   * @returns Detailed version information object
   */
  getVersionInfo: async (): Promise<ApiResult<VersionInfo>> => apiCall<Wire.VersionInfoDto>('get_version_info'),


  /**
   * Stores a HuggingFace authentication token securely in OS keyring.
   * Token must start with "hf_" and be at least 35 characters.
   *
   * @param token - HuggingFace API token
   * @returns Void on success
   */
  setHuggingFaceToken: async (token: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_huggingface_token', { token }),

  /**
   * Gets HuggingFace token status (whether it's set).
   * Does not return the actual token value for security.
   *
   * @returns Token status object
   */
  getHuggingFaceTokenStatus: async (): Promise<ApiResult<HfTokenStatus>> =>
    apiCall<Wire.HfTokenStatus>('get_huggingface_token_status'),

  /**
   * Gets the actual HuggingFace token value.
   * Should only be used internally for download authentication.
   *
   * @returns Token string or null if not set
   */
  getHuggingFaceToken: async (): Promise<ApiResult<string | null>> =>
    apiCall<string | null>('get_huggingface_token'),

  /**
   * Deletes the stored HuggingFace token from OS keyring.
   *
   * @returns Void on success
   */
  deleteHuggingFaceToken: async (): Promise<ApiResult<void>> =>
    apiCall<void>('delete_huggingface_token'),


  /**
   * Lists all batch jobs with pagination support.
   * Returns batch job history ordered by creation date (newest first).
   *
   * @param limit - Max jobs to return (default: 20)
   * @param offset - Skip N jobs for pagination (default: 0)
   * @returns Array of batch job summaries
   */
  listBatchJobs: async (limit?: number, offset?: number): Promise<ApiResult<BatchJobSummary[]>> => {
    const result = await apiCall<Wire.ListBatchJobsResponseDto>('get_batch_history', { request: { limit, offset } });
    if (!result.ok) {
      return result;
    }
    return {
      ok: true,
      data: (result.data.jobs || []).map(normalizeBatchSummary),
    };
  },

  /**
   * Deletes a batch job and all its associated items.
   * Permanent operation - cannot be undone.
   *
   * @param jobId - ID of batch job to delete
   * @returns Void on success
   */
  deleteBatchJob: async (jobId: string): Promise<ApiResult<void>> => {
    const result = await apiCall<Wire.DeleteBatchJobResponseDto>('delete_batch_job', { jobId });
    if (!result.ok) {
      return result;
    }
    if (!result.data.success) {
      return { ok: false, error: 'Failed to delete batch job' };
    }
    return { ok: true, data: undefined };
  },

  /**
   * Retries failed items through the matching importer. File retries update
   * their original job; an optional path replaces one selected failed file.
   *
   * @param jobId - ID of job with failed items
   * @returns New job ID for the retry operation
   */
  retryFailedBatchItems: async (jobId: string, itemId?: string, replacementPath?: string): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.RetryFailedItemsResponseDto>('retry_failed_items', { jobId, itemId, replacementPath });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.newJobId };
  },

  /**
   * Gets detailed status of a batch job including all items.
   * Returns comprehensive information about job progress and item statuses.
   *
   * @param jobId - ID of batch job to query
   * @returns Detailed batch job status with all items
   */
  getBatchJobStatus: async (jobId: string): Promise<ApiResult<BatchJobStatus>> => {
    const result = await apiCall<Wire.BatchJobStatusDto>('get_batch_status', { request: { jobId } });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: normalizeBatchStatus(result.data) };
  },

  /**
   * Cancels an ongoing batch job.
   * Stops processing and marks job as cancelled.
   *
   * @param jobId - ID of batch job to cancel
   * @returns Number of items cancelled
   */
  cancelBatchJob: async (jobId: string): Promise<ApiResult<number>> => {
    const result = await apiCall<Wire.CancelBatchJobResponseDto>('cancel_batch', { request: { jobId } });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.cancelledCount };
  },

  /**
   * Starts a batch import of files from disk.
   * Indexes multiple files in a single batch operation with progress tracking.
   *
   * @param filePaths - Array of absolute file paths to import
   * @returns Batch job ID for tracking progress
   */
  startBatchFileImport: async (filePaths: string[]): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchFileImportResponseDto>('batch_import_files', {
      request: { filePaths },
    });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },

  /**
   * Starts a batch import of URLs.
   * Fetches and indexes multiple URLs in a single batch operation with progress tracking.
   *
   * @param urls - Array of URLs to import
   * @returns Batch job ID for tracking progress
   */
  startBatchUrlImport: async (
    urls: string[],
    options?: { extractArticle?: boolean }
  ): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchUrlImportResponseDto>('batch_import_urls', {
      request: {
        urls,
        options: options ? { extractArticle: options.extractArticle } : null,
      },
    });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },


  /**
   * Extracts mentions (wikilinks, @-mentions) from document content.
   * Parses syntax like [[page]] and @person to create bidirectional links.
   *
   * @param documentId - Internal document identifier
   * @param content - Document content to parse
   * @returns Array of extracted mention objects
   */
  extractMentions: async (documentId: string, content: string): Promise<ApiResult<ExtractMentionsResponse>> => apiCall<Wire.ExtractMentionsResultDto>('extract_mentions', { documentId, content }),

  /**
   * Searches for mentions matching a query.
   * Finds mentions by name or partial name match.
   *
   * @param query - Search query for mention names
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching mention objects
   */
  searchMentions: async (query: string, limit?: number): Promise<ApiResult<SearchMentionsResponse>> => apiCall<Wire.SearchMentionsResultDto>('search_mentions', { query, limit }),

  /**
   * Gets all mentions found in a specific document.
   * Returns outgoing links from this document to other entities.
   *
   * @param documentId - Internal document identifier
   * @returns Array of mention objects found in the document
   */
  getMentionsForDocument: async (documentId: string): Promise<ApiResult<GetMentionsForDocumentResult>> => apiCall<Wire.GetMentionsForDocumentResultDto>('get_mentions_for_document', { documentId }),

  /**
   * Gets backlinks for a mention (documents that reference it).
   * Returns all documents that mention this entity.
   *
   * @param mentionName - Name of the mention to find backlinks for
   * @returns Array of documents that reference this mention
   */
  getBacklinksForMention: async (mentionName: string): Promise<ApiResult<BacklinksResponse>> => apiCall<Wire.BacklinksResultDto>('get_backlinks_for_mention', { mentionName }),

  /**
   * Vault-wide type mix and recent growth.
   *
   * For surfaces that do not already hold the document list (Home, the
   * post-ingest sentence). The Library counts its own array so the counts and
   * the filter can never disagree.
   */
  getCorpusShape: async (): Promise<ApiResult<CorpusShapeDto>> =>
    apiCall<Wire.CorpusShapeDto>('get_corpus_shape'),

  /**
   * Conversations that have this document among their linked documents.
   *
   * @param documentId - The document to look up
   * @param limit - Maximum conversations to return (default 10, max 50)
   */
  listConversationsCitingDocument: async (
    documentId: string,
    limit?: number,
  ): Promise<ApiResult<CitingConversationDto[]>> =>
    apiCall<Wire.CitingConversationDto[]>('list_conversations_citing_document', {
      documentId,
      limit,
    }),

  /** The themes from the most recent clustering run. */
  listClusters: async (): Promise<ApiResult<ClusterDto[]>> =>
    apiCall<Wire.ClusterDto[]>('list_clusters'),

  /** Runs clustering over the vault and replaces the stored themes. */
  clusterVaultRun: async (): Promise<ApiResult<ClusterRunDto>> =>
    apiCall<Wire.ClusterRunDto>('cluster_vault_run'),

  /**
   * Gets mentions filtered by type.
   * Types include 'wikilink', 'person', 'hashtag', etc.
   *
   * @param mentionType - Type of mentions to retrieve
   * @returns Array of mentions of the specified type
   */
  getMentionsByType: async (mentionType: string): Promise<ApiResult<Mention[]>> => apiCall<Wire.MentionDto[]>('get_mentions_by_type', { mentionType }),

  /**
   * Creates a new mention entity.
   * Manually adds a mention that can be referenced in documents.
   *
   * @param name - Mention name/identifier
   * @param mentionType - Type of mention (e.g., 'person', 'concept')
   * @returns Created mention object
   */
  createMention: async (name: string, mentionType: string): Promise<ApiResult<Mention>> => apiCall<Wire.MentionDto>('create_mention', { name, mentionType }),

  /**
   * Deletes a mention entity.
   * Removes mention and all its backlink relationships.
   *
   * @param mentionId - Internal mention identifier
   * @returns Void on success
   */
  deleteMention: async (mentionId: string): Promise<ApiResult<void>> => apiCall<void>('delete_mention', { mentionId }),


  /**
   * Checks for application updates.
   * Queries the update server for newer versions.
   *
   * @returns Update information with available version and download URL
   */
  checkForUpdates: async (): Promise<ApiResult<UpdateInfo>> => apiCall<Wire.UpdateInfoDto>('check_for_updates'),


  /**
   * Generates a vector embedding for a text string.
   * Uses ONNX model to convert text into semantic vector representation.
   *
   * @param text - Text to convert to embedding
   * @returns Embedding as array of numbers (typically 384 or 768 dimensions)
   */
  generateEmbedding: async (text: string): Promise<ApiResult<number[]>> => apiCall<number[]>('generate_embedding', { text }),

  /**
   * Generates embeddings for multiple texts in a single batch.
   * More efficient than calling generateEmbedding repeatedly.
   *
   * @param texts - Array of text strings to embed
   * @returns Array of embeddings (2D array of numbers)
   */
  generateEmbeddingsBatch: async (texts: string[]): Promise<ApiResult<number[][]>> => apiCall<number[][]>('generate_embeddings_batch', { texts }),

  /**
   * Gets information about the loaded embedding model.
   * Returns model name, dimensions, and metadata.
   *
   * @returns Model information object
   */
  getEmbeddingModelInfo: async (): Promise<ApiResult<EmbeddingModelInfo>> => apiCall<Wire.ModelInfo>('get_embedding_model_info'),


  /**
   * Parses wikilinks from markdown text.
   * Extracts [[wikilink]] syntax and returns structured link objects.
   *
   * @param text - Markdown text to parse
   * @param sourcePath - Source file path for relative link resolution (optional)
   * @returns Array of parsed wikilink objects
   */
  parseWikilinks: async (text: string, sourcePath?: string): Promise<ApiResult<ParsedLinksResponse>> => apiCall<Wire.ParsedLinksResponse>('parse_wikilinks', { text, sourcePath }),

  /**
   * Extracts the document title from content.
   * Looks for first H1 heading or YAML frontmatter title.
   *
   * @param content - Document content (markdown)
   * @returns Extracted title string or null if not found
   */
  extractDocumentTitle: async (content: string): Promise<ApiResult<string | null>> => apiCall<string | null>('extract_document_title', { content }),

  /**
   * Resolves a wikilink target to an absolute file path.
   * Handles relative paths, aliases, and folder-relative links.
   *
   * @param target - Wikilink target (e.g., 'Page Name' or '../folder/page')
   * @param sourcePath - Source file path for relative resolution
   * @param availableDocuments - Candidate documents to resolve against
   * @returns Resolution and matching document, if found
   */
  resolveWikilink: async (target: string, sourcePath: string, availableDocuments: Wire.DocumentRefDto[]): Promise<ApiResult<Wire.ResolveLinkResponse>> => apiCall<Wire.ResolveLinkResponse>('resolve_wikilink', { target, sourcePath, availableDocuments }),

  /**
   * Extracts and resolves all links from document content.
   * Combines parsing and resolution into single operation.
   *
   * @param content - Document content to parse
   * @param sourcePath - Source file path for resolution
   * @returns Object with extracted and resolved links
   */
  extractAndResolveLinks: async (content: string, documentId: string): Promise<ApiResult<ExtractAndResolveLinksResponse>> => apiCall<Wire.ExtractAndResolveResponseDto>('extract_and_resolve_links', { content, documentId }),


  /**
   * Creates a new conversation for multi-turn Q&A with context.
   * Initializes a conversation thread that maintains context across multiple
   * question-answering interactions.
   *
   * @param title - Human-readable conversation title
   * @param modelName - LLM model to use (e.g., "gpt-4", "claude-3-opus")
   * @param systemPrompt - Optional system instructions to guide LLM behavior
   * @returns Response with created conversation and status
   */
  createConversation: async (
    title: string,
    modelName: string,
    systemPrompt?: string
  ): Promise<ApiResult<CreateConversationResponse>> =>
    apiCall<Wire.CreateConversationResponseDto>('create_conversation', {
    request: {
      title,
      modelName,
      systemPrompt,
    },
  }),

  /**
   * Lists all conversations with pagination support.
   * Returns conversations ordered by most recently updated first.
   *
   * @param limit - Maximum number of conversations to return (default: 100)
   * @param offset - Number of conversations to skip for pagination (default: 0)
   * @returns Response with conversations and total count
   */
  listConversations: async (
    limit?: number,
    offset?: number
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_conversations', {
    query: {
      limit,
      offset,
    },
  }),

  /**
   * Retrieves a single conversation by ID.
   * Returns conversation metadata without loading message history.
   *
   * @param conversationId - Unique identifier of the conversation to retrieve
   * @returns Response with conversation metadata or null if not found
   */
  getConversation: async (
    conversationId: string
  ): Promise<ApiResult<GetConversationResponse>> =>
    apiCall<Wire.GetConversationResponseDto>('get_conversation', {
      request: {
        conversationId,
      },
    }),

  /**
   * Retrieves all messages for a conversation.
   * Returns complete message history in chronological order.
   *
   * @param conversationId - Unique identifier of the conversation
   * @returns Response with messages and total count
   */
  getConversationMessages: async (
    conversationId: string
  ): Promise<ApiResult<GetConversationMessagesResponse>> =>
    apiCall<Wire.GetConversationMessagesResponseDto>('get_conversation_messages', {
      request: {
        conversationId,
      },
    }),

  /**
   * Renames a conversation with a new title.
   * Updates the conversation's title (sanitized for security).
   *
   * @param conversationId - Unique identifier of the conversation to rename
   * @param newTitle - New title for the conversation
   * @returns Response with status message
   */
  renameConversation: async (
    conversationId: string,
    newTitle: string
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('rename_conversation', {
    request: {
      conversationId,
      newTitle,
    },
  }),

  /**
   * Sends a message to a conversation and receives an AI response.
   * Creates a new conversation if conversationId is null.
   *
   * @param conversationId - ID of existing conversation, or null to create new one
   * @param message - User's message text
   * @returns Response containing conversation ID, all messages, and context usage
   */
  chatWithConversation: async (
    conversationId: string | null,
    message: string,
    toolPreferences?: ToolPreferences,
    requestId?: string
  ): Promise<ApiResult<Wire.ChatResponse>> => apiCall<Wire.ChatResponse>('chat_with_conversation', {
      conversationId,
      message,
      requestId,
      toolPreferences,
    }),

  /**
   * Cancels active model generation for a conversation if one is currently in-flight.
   */
  cancelConversationGeneration: async (
    conversationId: string,
    requestId: string
  ): Promise<ApiResult<void>> => {
    const response = await apiCall<Wire.ChatResponse>('chat_with_conversation', {
      conversationId,
      message: '',
      requestId,
      cancelOnly: true,
    });
    if (!response.ok) {
      return response as unknown as ApiResult<void>;
    }
    return { ok: true, data: undefined };
  },

  /**
   * Deletes a conversation and all associated messages.
   * This operation is irreversible.
   *
   * @param conversationId - Unique identifier of the conversation to delete
   * @returns Response with status message
   */
  deleteConversation: async (
    conversationId: string
  ): Promise<ApiResult<DeleteConversationResponse>> =>
    apiCall<Wire.DeleteConversationResponseDto>('delete_conversation', {
      request: {
        conversationId,
      },
    }),

  /**
   * Creates a conversation space (environment) with optional defaults.
   */
  createConversationSpace: async (
    request: CreateConversationSpaceRequest
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('create_conversation_space', { request }),

  /**
   * Lists all conversation spaces.
   */
  listConversationSpaces: async (): Promise<ApiResult<ConversationSpaceDto[]>> =>
    apiCall<Wire.ConversationSpaceDto[]>('list_conversation_spaces'),

  /**
   * Creates a journal notebook.
   */
  createJournal: async (
    request: CreateConversationJournalRequest
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('create_journal', { request }),

  /**
   * Lists all journals.
   */
  listJournals: async (): Promise<ApiResult<ConversationJournalDto[]>> =>
    apiCall<Wire.ConversationJournalDto[]>('list_journals'),

  /**
   * Lists members for a specific conversation space.
   */
  listConversationSpaceMembers: async (
    spaceId: string
  ): Promise<ApiResult<ConversationSpaceMemberDto[]>> =>
    apiCall<Wire.ConversationSpaceMemberDto[]>('list_conversation_space_members', {
      spaceId,
    }),

  /**
   * Adds/updates a conversation space member and role.
   */
  upsertConversationSpaceMember: async (
    request: UpsertConversationSpaceMemberRequest
  ): Promise<ApiResult<ConversationSpaceMemberDto>> =>
    apiCall<Wire.ConversationSpaceMemberDto>('upsert_conversation_space_member', { request }),

  /**
   * Removes a member from a conversation space.
   */
  removeConversationSpaceMember: async (
    request: RemoveConversationSpaceMemberRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('remove_conversation_space_member', { request }),

  /**
   * Updates a conversation space.
   */
  updateConversationSpace: async (
    request: UpdateConversationSpaceRequest
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('update_conversation_space', { request }),

  /**
   * Archives/unarchives a conversation space.
   */
  archiveConversationSpace: async (
    request: ArchiveConversationSpaceRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_conversation_space', { request }),

  // The `*ConversationThread` wrappers that used to live here were removed:
  // the feature was renamed thread → space on the backend, so they invoked
  // `create_conversation_thread` and friends, none of which are registered
  // Tauri commands. Use the `*ConversationSpace` functions above.

  /**
   * Updates a journal notebook.
   */
  updateJournal: async (
    request: UpdateConversationJournalRequest
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('update_journal', { request }),

  /**
   * Archives/unarchives a journal notebook.
   */
  archiveJournal: async (
    request: ArchiveConversationJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_journal', { request }),

  /**
   * Permanently deletes a journal notebook.
   */
  deleteJournal: async (
    request: DeleteConversationJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_journal', { request }),

  /**
   * Moves a conversation to a space.
   */
  moveConversationToSpace: async (
    request: MoveConversationToSpaceRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('move_conversation_to_space', { request }),

  /**
   * Adds a conversation to a journal without changing its owning space.
   */
  addConversationToJournal: async (
    request: AddConversationToJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_conversation_to_journal', { request }),

  /**
   * Removes a conversation from a journal entry deck.
   */
  removeConversationFromJournal: async (
    request: RemoveConversationFromJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('remove_conversation_from_journal', { request }),

  /**
   * Sets the "saved" state for a conversation.
   */
  setConversationSaved: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_saved', { request }),

  /**
   * Sets the "bookmarked" state for a conversation.
   */
  setConversationBookmarked: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_bookmarked', { request }),

  /**
   * Sets the "pinned" state for a conversation.
   */
  setConversationPinned: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_pinned', { request }),

  /**
   * Sets the "archived" state for a conversation.
   */
  setConversationArchived: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_archived', { request }),

  /**
   * Deletes a specific message from a conversation.
   */
  deleteConversationMessage: async (
    request: DeleteConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_conversation_message', { request }),

  /**
   * Bookmarks a specific message in a conversation (upsert by message).
   */
  bookmarkConversationMessage: async (
    request: BookmarkConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('bookmark_conversation_message', { request }),

  /**
   * Removes a message bookmark from a conversation.
   */
  unbookmarkConversationMessage: async (
    request: UnbookmarkConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('unbookmark_conversation_message', { request }),

  /**
   * Lists message-level bookmarks with optional conversation and text filters.
   */
  listMessageBookmarks: async (
    query?: ListMessageBookmarksQuery
  ): Promise<ApiResult<ListMessageBookmarksResponse>> =>
    apiCall<Wire.ListMessageBookmarksResponseDto>('list_message_bookmarks', {
      query: query ?? {},
    }),

  /**
   * Lists conversations for the explorer with filters and text search.
   */
  listConversationsExplorer: async (
    query?: ListConversationsExplorerQuery
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_conversations_explorer', {
      query: query ?? {},
    }),

  /**
   * Lists conversations included in a journal (additive membership).
   */
  listJournalConversations: async (
    query: ListJournalConversationsQuery
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_journal_conversations', {
      query,
    }),

  /**
   * The documents a chat in this space may read, newest first.
   *
   * `spaceId` of `null` means General. The backend answers from the same
   * allow-list retrieval uses, so what this offers is exactly what a turn can
   * search — never a superset. `limit` is clamped to 50 there.
   */
  listSpaceDocuments: async (
    spaceId: string | null,
    query: string,
    limit: number
  ): Promise<ApiResult<SpaceDocument[]>> =>
    apiCall<Wire.SpaceDocumentDto[]>('list_space_documents', {
      spaceId,
      query,
      limit,
    }),

  /**
   * Lists documents currently linked to a conversation context.
   */
  listConversationLinkedDocuments: async (
    conversationId: string
  ): Promise<ApiResult<ConversationLinkedDocumentDto[]>> =>
    apiCall<Wire.ConversationLinkedDocumentDto[]>('list_conversation_linked_documents', {
      conversationId,
    }),

  /**
   * Removes a linked document reference from a conversation.
   */
  removeConversationLinkedDocument: async (
    conversationId: string,
    documentId: string
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('remove_conversation_linked_document', {
      conversationId,

      documentId,
    }),

  /**
   * Adds or updates a non-ingested web source linked to a conversation.
   */
  addConversationWebSource: async (
    conversationId: string,
    url: string,
    options?: {
      title?: string;
      excerpt?: string;
      relevanceScore?: number;
    }
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_conversation_web_source', {
      conversationId,
      url,
      title: options?.title,
      excerpt: options?.excerpt,

      relevanceScore: options?.relevanceScore,
    }),

  /**
   * Lists non-ingested web sources currently linked to a conversation context.
   */
  listConversationWebSources: async (
    conversationId: string
  ): Promise<ApiResult<ConversationWebSourceDto[]>> =>
    apiCall<Wire.ConversationWebSourceDto[]>('list_conversation_web_sources', {
      conversationId,
    }),

  /**
   * Removes a linked web source from a conversation context.
   */
  removeConversationWebSource: async (
    conversationId: string,
    sourceId: string
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('remove_conversation_web_source', {
      conversationId,

      sourceId,
    }),

  /**
   * Lists all spaces currently assigned to a document.
   */
  listDocumentSpaceMemberships: async (
    documentId: string
  ): Promise<ApiResult<DocumentSpaceMembershipDto[]>> =>
    apiCall<Wire.DocumentSpaceMembershipDto[]>('list_document_space_memberships', {
      documentId,
    }),

  /**
   * Assigns or removes a document from a space scope.
   */
  setDocumentSpaceMembership: async (
    documentId: string,
    spaceId: string,
    assigned: boolean
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_document_space_membership', {
      documentId,

      spaceId,
      assigned,
    }),

  /**
   * Synthesizes multiple conversations into a structured journal summary.
   */
  synthesizeJournalEntries: async (
    request: SynthesizeJournalEntriesRequest
  ): Promise<ApiResult<SynthesizeJournalEntriesResponse>> =>
    apiCall<Wire.SynthesizeJournalEntriesResponseDto>('synthesize_journal_entries', {
      request,
    }),

  /**
   * Deletes every message after `messageId` (and `messageId` itself when
   * `inclusive`). Returns the remaining messages so callers can replace their
   * cache rather than reason about what was removed.
   */
  truncateConversationAfter: async (
    conversationId: string,
    messageId: string,
    inclusive = false
  ): Promise<ApiResult<{
    conversationId: string;
    deletedCount: number;
    messages: Wire.MessageDto[];
  }>> =>
    apiCall('truncate_conversation_after', {
      request: { conversationId, messageId, inclusive },
    }),

  /** Creates a sibling conversation copying messages up to `upToMessageId`. */
  forkConversation: async (
    conversationId: string,
    upToMessageId?: string
  ): Promise<ApiResult<{ conversation: Conversation; copiedMessageCount: number }>> =>
    apiCall('fork_conversation', {
      request: { conversationId, upToMessageId },
    }),

  /**
   * Re-runs the last user message. Streams over `llm-stream` exactly like
   * `chatWithConversation`; the user message is not duplicated.
   */
  regenerateResponse: async (
    conversationId: string,
    toolPreferences?: ToolPreferences,
    requestId?: string
  ): Promise<ApiResult<Wire.ChatResponse>> => apiCall<Wire.ChatResponse>('regenerate_response', {
      conversationId,
      toolPreferences,
      requestId,
    }),

  /**
   * Folds the conversation's oldest messages into an LLM summary. The raw
   * messages stay in the history; only the LLM context switches to the
   * summary. Returns the applied compaction record.
   */
  compactConversation: async (
    conversationId: string,
    keepRecentMessages?: number
  ): Promise<ApiResult<{ compaction: Wire.CompactionRecordDto }>> =>
    apiCall('compact_conversation', {
      request: { conversationId, keepRecentMessages: keepRecentMessages ?? null },
    }),

  /**
   * Assigns or removes multiple documents from a space scope.
   */
  setDocumentsSpaceMembership: async (
    documentIds: string[],
    spaceId: string,
    assigned: boolean
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_documents_space_membership', {
      documentIds,

      spaceId,
      assigned,
    }),


  /**
   * Executes an LLM function call with security checks.
   * Executes a function from the registry with rate limiting and validation.
   *
   * @param name - Function name to execute
   * @param args - Function arguments as JSON object
   * @returns Execution result with success flag and data/error
   */
  executeFunction: async (
    name: string,
    args: Record<string, Wire.JsonValue>
  ): Promise<ApiResult<FunctionResult>> => apiCall<FunctionResult>('execute_function', {
    call: {
      id: crypto.randomUUID(),
      name,
      arguments: args,
    },
  }),

  /**
   * Lists all available tool definitions for LLM function calling.
   * Returns metadata for all registered functions including schemas.
   *
   * @returns Array of tool definitions with names, descriptions, and input schemas
   */
  listAvailableFunctions: async (): Promise<ApiResult<FunctionDefinition[]>> => apiCall<FunctionDefinition[]>('list_available_functions'),


  /**
   * Starts downloading a model from a URL.
   * Supports resumable downloads with checksum verification.
   *
   * @param request - Download configuration including URL, destination, and metadata
   * @returns Download ID for tracking progress
   *
   * @example
   * const result = await VaultAPI.startModelDownload({
   *   url: 'https://huggingface.co/model/file.gguf',
   *   destination: '/path/to/models/model.gguf',
   *   model_name: 'My Model',
   *   model_id: 'my-model-id'
   * });
   */
  startModelDownload: async (request: StartDownloadRequest): Promise<ApiResult<string>> =>
    apiCall<string>('start_model_download', { request }),

  /**
   * Pauses an ongoing download.
   * Download can be resumed later from where it stopped.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  pauseDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('pause_download', { id: downloadId }),

  /**
   * Resumes a paused download.
   * Continues from the last downloaded byte.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  resumeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('resume_download', { id: downloadId }),

  /**
   * Cancels an ongoing or paused download.
   * Deletes partial download file.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  cancelDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('cancel_download', { id: downloadId }),

  /**
   * Retries a failed download.
   * Attempts to restart a download that encountered an error.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  retryDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('retry_download', { id: downloadId }),

  /**
   * Removes a download from the download list.
   * Deletes download record and any partial files.
   *
   * @param downloadId - Unique download identifier
   * @returns Void on success
   */
  removeDownload: async (downloadId: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_download', { id: downloadId }),

  /**
   * Clears all completed downloads from the download list.
   * Removes download records for successful downloads.
   *
   * @returns Number of downloads cleared
   */
  clearCompletedDownloads: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_completed_downloads'),

  /**
   * Gets the current status and progress of a download.
   * Returns null if download ID doesn't exist.
   *
   * @param downloadId - Unique download identifier
   * @returns Download status with progress information or null
   */
  getDownloadStatus: async (downloadId: string): Promise<ApiResult<DownloadStatus | null>> =>
    apiCall<Wire.DownloadStatusResponse | null>('get_download_status', { id: downloadId }).then(result => result.ok ? { ok: true, data: result.data === null ? null : normalizeDownloadStatus(result.data) } : result),

  /**
   * Lists all downloads (active, paused, completed, failed).
   * Returns comprehensive list for download management UI.
   *
   * @returns Array of all download statuses
   */
  listDownloads: async (): Promise<ApiResult<DownloadStatus[]>> =>
    apiCall<Wire.DownloadStatusResponse[]>('list_downloads').then(result => result.ok ? { ok: true, data: result.data.map(normalizeDownloadStatus) } : result),


  /**
   * Gets all downloaded models with metadata.
   * Includes both chat and embedding models from the downloaded_models table.
   *
   * NOTE: Gateway Pattern returns structured data directly (no JSON string parsing needed).
   *
   * @returns Array of downloaded model records
   */
  getDownloadedModels: async (): Promise<ApiResult<DownloadedModel[]>> =>
    apiCall<Wire.DownloadedModelResponse[]>('get_models_with_metadata'),

  /**
   * Checks if a model is already downloaded.
   * Queries the downloaded_models table by model_id.
   *
   * @param modelId - Model identifier to check
   * @returns True if model exists in database
   */
  isModelDownloaded: async (modelId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_model_already_downloaded', { modelId }),

  /**
   * Sets the active chat model.
   * Only one model can be active for chat at a time.
   *
   * @param modelId - Model identifier to set as active
   * @returns Void on success
   */
  setActiveChatModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_chat_model', { modelId }),

  /**
   * Warms up the currently active chat model by preloading it into memory.
   * Useful after switching models so the first chat response is faster.
   *
   * @returns Void on success
   */
  warmUpActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_chat_model'),

  /**
   * Eagerly load the currently active utility model into memory.
   *
   * Utility models (HyDE expansion, intent routing, follow-up classifier)
   * pay a 60-120s cold-start cost on first use. Calling this from the
   * UI pays that cost up-front so the user's first chat turn doesn't
   * carry it. No-op (returns Ok) when no utility model is configured.
   */
  warmUpActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('warm_up_active_utility_model'),

  /**
   * Gets the currently active chat model.
   * Returns null if no model is set as active.
   *
   * @returns Active chat model or null
   */
  getActiveChatModel: async (): Promise<ApiResult<DownloadedModel | null>> =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_chat_model'),

  /**
   * Sets the active embedding model.
   * Only one model can be active for embeddings at a time.
   *
   * @param modelId - Model identifier to set as active
   * @returns Void on success
   */
  setActiveEmbeddingModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_embedding_model', { modelId }),

  /**
   * Sets the active utility model (used for HyDE, routing, intent classification).
   * Only one model can be active for utility at a time.
   *
   * @param modelId - Model identifier to set as active utility model
   * @returns Void on success
   */
  setActiveUtilityModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('set_active_utility_model', { modelId }),

  /**
   * Clears the active utility model — reverts HyDE/router back to the chat model.
   *
   * @returns Void on success
   */
  clearActiveUtilityModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_utility_model'),

  /**
   * Gets the currently active embedding model.
   * Returns null if no model is set as active.
   *
   * @returns Active embedding model or null
   */
  getActiveEmbeddingModel: async (): Promise<ApiResult<DownloadedModel | null>> =>
    apiCall<Wire.DownloadedModelResponse | null>('get_active_embedding_model'),

  /**
   * Absolute path of the local models folder (created on first call).
   */
  getModelDownloadPath: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_model_download_path'),

  getActiveModels: async (): Promise<ApiResult<{
    chat_model: DownloadedModel | null;
    embedding_model: DownloadedModel | null;
  }>> =>
    apiCall<Wire.ActiveModels>('get_active_models'),

  /**
   * Deletes a downloaded model record and optionally its file.
   * Removes from database and optionally from disk.
   *
   * @param modelId - Model identifier to delete
   * @param deleteFile - Whether to delete the file from disk (default: false)
   * @returns Void on success
   */
  deleteDownloadedModel: async (modelId: string, deleteFile: boolean = false): Promise<ApiResult<void>> =>
    apiCall<void>('delete_downloaded_model_and_file', { modelId, deleteFile }),


  /**
   * Gets system hardware capabilities for model selection.
   * Detects GPU, RAM, CPU cores, and platform to recommend compatible models.
   *
   * @returns System capabilities including GPU availability and resources
   */
  getSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  /**
   * Downloads a model from the catalog.
   * Starts a download job that can be tracked via download management.
   *
   * @param modelId - Model identifier to download
   * @returns Download ID for tracking progress
   */
  downloadModel: async (modelId: string): Promise<ApiResult<DownloadModelResponse>> =>
    apiCall<Wire.DownloadModelResponse>('download_model', { modelId }),

  /**
   * Rescan the vault folder for external `.md` edits the watcher may have
   * missed. No-op if vault is disabled or watch toggle is off (backend
   * gates). Returns counts of files scanned and imported.
   *
   * Called from the focus-rescan hook whenever the Tauri window regains
   * focus — belt-and-suspenders for the fs watcher.
   */
  rescanVault: async (): Promise<
    ApiResult<{ scanned: number; imported: number; deleted: number }>
  > =>
    apiCall<Wire.RescanSummary>('rescan_vault'),

  /**
   * Starts a batch file import operation for indexing multiple files
   * @param filePaths - Array of absolute file paths to index
   * @returns Operation ID for tracking progress
   */
  batchFileImport: async (filePaths: string[], spaceId?: string, indexing?: Wire.FileIndexingOptionsDto): Promise<ApiResult<string>> => {
    const result = await apiCall<Wire.StartBatchFileImportResponseDto>('batch_import_files', {
      request: { filePaths, spaceId, indexing },
    });
    if (!result.ok) {
      return result;
    }
    return { ok: true, data: result.data.jobId };
  },

  /**
   * Deletes a model file from disk.
   * Removes the model file but may leave database record.
   *
   * @param modelId - Model identifier to delete
   * @returns Void on success
   */
  deleteModel: async (modelId: string, deleteFile = true): Promise<ApiResult<void>> =>
    apiCall<void>('delete_model', { modelId, deleteFile }),


  /**
   * Detects system capabilities for model compatibility.
   * Similar to getSystemCapabilities but may include more detailed info.
   *
   * @returns Detailed system capabilities
   */
  detectSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<Wire.SystemCapabilitiesResponse>('detect_system_capabilities'),

  /**
   * Gets all models compatible with the current system.
   * Filters catalog by RAM, GPU, and other requirements.
   *
   * @returns Array of compatible models
   */
  getCompatibleModels: async (category?: string): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<Wire.ModelRecommendationDto[]>('get_compatible_models', category ? { category } : {}),

  /**
   * Gets all recommended models across all tasks.
   * Returns best options for chat, embedding, etc.
   *
   * @returns Array of recommended models by task type
   */
  getAllRecommendedModels: async (): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<Wire.ModelRecommendationDto[]>('get_all_recommended_models'),

  /**
   * Searches the model catalog by name or description.
   * Supports fuzzy matching for model discovery.
   *
   * @param query - Search query string
   * @returns Array of matching models with relevance scores
   */
  searchModelCatalog: async (request: SearchModelCatalogRequest): Promise<ApiResult<ModelSearchResult[]>> =>
    apiCall<Wire.ModelSearchResultDto[]>('search_model_catalog', { request }),

  /**
   * Refreshes the model catalog from remote source.
   * Updates available models and metadata from HuggingFace.
   *
   * @returns Void on success
   */
  refreshModelCatalog: async (): Promise<ApiResult<void>> =>
    apiCall<void>('refresh_model_catalog'),

  /**
   * Clears the cached model catalog data.
   * Forces next request to fetch fresh catalog.
   *
   * @returns Void on success
   */
  clearModelCatalogCache: async (): Promise<ApiResult<number>> =>
    apiCall<number>('clear_model_catalog_cache'),

  /**
   * Gets statistics about the model catalog cache.
   * Returns cache usage metrics and effectiveness.
   *
   * @returns Cache statistics
   */
  getModelCatalogStats: async (): Promise<ApiResult<ModelCatalogCacheStats>> =>
    apiCall<Wire.ModelCatalogStats>('get_model_catalog_stats'),


  /**
   * Clears the active chat model selection.
   * Deactivates any currently selected chat model.
   *
   * @returns Void on success
   */
  clearActiveChatModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_chat_model'),

  /**
   * Clears the active embedding model selection.
   * Deactivates any currently selected embedding model.
   *
   * @returns Void on success
   */
  clearActiveEmbeddingModel: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_active_embedding_model'),



  /**
   * Gets statistics about LLM function calling usage.
   * Returns execution counts, success rates, and timing info.
   *
   * @returns Function calling statistics
   */
  getFunctionStats: async (): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall<Record<string, unknown>>('get_function_stats'),



  /**
   * Transcribes an audio file on-device and returns timestamped segments.
   * Explicit re-run; normal ingest transcribes automatically.
   *
   * @param path - Absolute path to an mp3/wav/m4a/flac/ogg file
   * @returns Transcript with segments and rendered text
   */
  transcribeFile: async (path: string): Promise<ApiResult<Transcript>> =>
    apiCall<Wire.TranscriptDto>('transcribe_file', { path }),

  /**
   * Reports whether a transcription model is downloaded.
   *
   * @returns TranscriptionStatus with the model name when ready
   */
  getTranscriptionStatus: async (): Promise<ApiResult<TranscriptionStatus>> =>
    apiCall<Wire.TranscriptionStatusDto>('get_transcription_status'),

};

export { VaultAPI };
export default VaultAPI;
