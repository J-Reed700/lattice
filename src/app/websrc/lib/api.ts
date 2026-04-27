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

import type {
  SearchOptions,
  SearchResult,
  IndexProgress,
  IndexStatus,
  IndexingStats,
  IndexedFolder,
  IndexingActivity,
  IndexFileResponse,
  AppConfig,
  RecentDocument,
  DocumentMetadata,
  ApiResult,
  WebIngestResponse,
  BatchIngestSummary,
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
  TagWithCount,
  FavoriteDocument,
  // Tag Request/Response types
  CreateTagRequest,
  CreateTagResponse,
  ListTagsResponse,
  RenameTagRequest,
  RenameTagResponse,
  SetTagColorRequest,
  SetTagColorResponse,
  DeleteTagRequest,
  AddTagToDocumentRequest,
  RemoveTagFromDocumentRequest,
  GetDocumentTagsRequest,
  DocumentTagsResponse,
  SearchTagsRequest,
  SearchTagsResponse,
  GetTagStatsRequest,
  TagStatsResponse,
  FindDocumentsByTagRequest,
  TaggedDocumentsResponse,
  QAResponse,
  LLMHealthStatus,
  CacheStats,
  CacheMetrics,
  LLMCacheStats,
  HealthStatus,
  SystemStats,
  Mention,
  MentionWithContext,
  ExtractMentionsResponse,
  SearchMentionsResponse,
  BacklinksResponse,
  FileMetadata,
  UpdateInfo,
  MetricsSnapshot,
  EmbeddingModelInfo,
  ParsedLinksResponse,
  GenerateTagsBatchResponse,
  ExtractAndResolveLinksResponse,
  OpenFileResponseDto,
  // Conversation types (Wave 2B)
  ConversationMessage,
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
  ConversationThreadDto,
  CreateConversationThreadRequest,
  UpdateConversationThreadRequest,
  DeleteConversationThreadRequest,
  ArchiveConversationThreadRequest,
  MoveConversationToThreadRequest,
  ListConversationThreadsRequest,
  DeleteConversationMessageRequest,
  BookmarkConversationMessageRequest,
  UnbookmarkConversationMessageRequest,
  ListMessageBookmarksQuery,
  ListMessageBookmarksResponse,
  ConversationLinkedDocumentDto,
  ConversationWebSourceDto,
  DocumentSpaceMembershipDto,
  // Function calling types (Wave 2B)
  FunctionDefinition,
  FunctionResult,
  // Backup and export types (Wave 2D)
  BackupInfo,
  // Web content types (Wave 2D)
  // Batch and credentials types (Wave 3)
  BatchJobSummary,
  BatchJobStatus,
  CancelBatchJobResponse,
  DeleteBatchJobResponse,
  HfTokenStatus,
  ListBatchJobsResponse,
  RetryFailedItemsResponse,
  // Model management types (Wave 4B)
  SystemCapabilities,
  ModelInfo,
  ModelSearchResult,
  ModelRecommendation,
  ModelCatalogCacheStats,
} from '../types';
import type { DownloadModelResponse } from '../types/download';
import type { DownloadedModel } from '../types/downloadedModels';
import type { StartDownloadRequest, DownloadStatus } from '../types/downloads';
import type { SearchModelCatalogRequest } from '../types/modelCatalog';

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
  get_active_chat_model: { domain: 'model', command: 'get_active_chat_model' },
  get_active_models: { domain: 'model', command: 'get_active_models' },
  delete_downloaded_model_and_file: { domain: 'model', command: 'delete_model' },
  get_active_embedding_model: { domain: 'model', command: 'get_active_embedding_model' },
  set_active_embedding_model: { domain: 'model', command: 'set_active_embedding_model' },
  download_model_command: { domain: 'model', command: 'download_model_command' },
  detect_system_capabilities: { domain: 'model', command: 'detect_system_capabilities' },
  get_all_recommended_models: { domain: 'model', command: 'get_all_recommended_models' },
  get_model_catalog_stats: { domain: 'model', command: 'get_model_catalog_stats' },

  // Search domain
  search_documents: { domain: 'search', command: 'search_documents' },
  search_fast: { domain: 'search', command: 'search_fast' },
  semantic_search: { domain: 'search', command: 'semantic_search' },
  hybrid_search: { domain: 'search', command: 'hybrid_search' },
  find_similar: { domain: 'search', command: 'find_similar' },
  search_with_recency: { domain: 'search', command: 'search_with_recency' },
  batch_search: { domain: 'search', command: 'batch_search' },

  // File domain
  open_file: { domain: 'file', command: 'open_file' },
  open_file_by_id: { domain: 'file', command: 'open_file_by_id' },
  get_file_path_by_id: { domain: 'file', command: 'get_file_path_by_id' },
  get_file_metadata: { domain: 'file', command: 'get_file_metadata' },
  read_file_content: { domain: 'file', command: 'read_file_content' },
  read_file_bytes: { domain: 'file', command: 'read_file_bytes' },
  show_in_folder: { domain: 'file', command: 'show_in_folder' },
  get_indexed_folders: { domain: 'file', command: 'get_indexed_folders' },
  remove_indexed_folder: { domain: 'file', command: 'remove_indexed_folder' },
  get_indexing_activities: { domain: 'file', command: 'get_indexing_activities' },

  // Health domain
  health_check: { domain: 'health', command: 'health_check' },
  get_system_stats: { domain: 'health', command: 'get_system_stats' },
  get_version: { domain: 'health', command: 'get_version' },

  // Config domain
  get_config: { domain: 'config', command: 'get_config' },
  save_config: { domain: 'config', command: 'save_config' },

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
  get_recent_documents: { domain: 'file', command: 'get_recent_documents' },
  get_document: { domain: 'file', command: 'get_document' },

  // Tag domain
  create_tag: { domain: 'tags', command: 'create_tag' },
  get_all_tags_with_counts: { domain: 'tags', command: 'get_all_tags_with_counts' },
  rename_tag: { domain: 'tags', command: 'rename_tag' },
  set_tag_color: { domain: 'tags', command: 'set_tag_color' },
  delete_tag: { domain: 'tags', command: 'delete_tag' },
  add_tag_to_document: { domain: 'tags', command: 'add_tag_to_document' },
  remove_tag_from_document: { domain: 'tags', command: 'remove_tag_from_document' },
  get_document_tags: { domain: 'tags', command: 'get_document_tags' },
  search_tags: { domain: 'tags', command: 'search_tags' },
  get_tag_stats: { domain: 'tags', command: 'get_tag_stats' },
  find_documents_by_tag: { domain: 'tags', command: 'find_documents_by_tag' },
  apply_tags: { domain: 'tags', command: 'apply_tags' },
  generate_tags_for_document: { domain: 'tags', command: 'generate_tags_for_document' },

  // QA domain
  ask_question: { domain: 'qa', command: 'ask_question_wrapper' },
  ask_question_stream: { domain: 'qa', command: 'ask_question_stream_wrapper' },
  check_llm_health: { domain: 'qa', command: 'check_llm_health_wrapper' },

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
  list_conversation_linked_documents: { domain: 'conversation', command: 'list_conversation_linked_documents' },
  add_conversation_linked_document: { domain: 'conversation', command: 'add_conversation_linked_document' },
  remove_conversation_linked_document: { domain: 'conversation', command: 'remove_conversation_linked_document' },
  add_conversation_web_source: { domain: 'conversation', command: 'add_conversation_web_source' },
  list_conversation_web_sources: { domain: 'conversation', command: 'list_conversation_web_sources' },
  remove_conversation_web_source: { domain: 'conversation', command: 'remove_conversation_web_source' },
  list_document_space_memberships: { domain: 'conversation', command: 'list_document_space_memberships' },
  set_document_space_membership: { domain: 'conversation', command: 'set_document_space_membership' },
  set_documents_space_membership: { domain: 'conversation', command: 'set_documents_space_membership' },
  synthesize_journal_entries: { domain: 'conversation', command: 'synthesize_journal_entries' },
  create_conversation_thread: { domain: 'conversation', command: 'create_conversation_thread' },
  list_conversation_threads: { domain: 'conversation', command: 'list_conversation_threads' },
  update_conversation_thread: { domain: 'conversation', command: 'update_conversation_thread' },
  archive_conversation_thread: { domain: 'conversation', command: 'archive_conversation_thread' },
  delete_conversation_thread: { domain: 'conversation', command: 'delete_conversation_thread' },
  move_conversation_to_thread: { domain: 'conversation', command: 'move_conversation_to_thread' },

  // Credentials domain
  set_api_key: { domain: 'credentials', command: 'set_api_key' },
  get_api_key: { domain: 'credentials', command: 'get_api_key' },
  delete_api_key: { domain: 'credentials', command: 'delete_api_key' },
  has_api_key: { domain: 'credentials', command: 'has_api_key' },
  clear_all_credentials: { domain: 'credentials', command: 'clear_all_credentials' },
  set_custom_endpoint: { domain: 'credentials', command: 'set_custom_endpoint' },
  get_custom_endpoint: { domain: 'credentials', command: 'get_custom_endpoint' },
  set_huggingface_token: { domain: 'huggingface', command: 'set_huggingface_token' },
  get_huggingface_token_status: { domain: 'huggingface', command: 'get_huggingface_token_status' },
  get_huggingface_token: { domain: 'huggingface', command: 'get_huggingface_token' },
  delete_huggingface_token: { domain: 'huggingface', command: 'delete_huggingface_token' },

  // Batch operations domain
  list_batch_jobs: { domain: 'batch', command: 'list_batch_jobs' },
  delete_batch_job: { domain: 'batch', command: 'delete_batch_job' },
  retry_failed_items: { domain: 'batch', command: 'retry_failed_items' },
  get_batch_job_status: { domain: 'batch', command: 'get_batch_job_status' },
  cancel_batch_job: { domain: 'batch', command: 'cancel_batch_job' },
  start_batch_file_import: { domain: 'batch', command: 'start_batch_file_import' },
  start_batch_url_import: { domain: 'batch', command: 'start_batch_url_import' },

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
  get_available_models: { domain: 'model', command: 'get_available_models' },
  get_recommended_models: { domain: 'model', command: 'get_recommended_models' },
  get_best_model: { domain: 'model', command: 'get_best_model' },
  is_model_downloaded: { domain: 'model', command: 'is_model_downloaded' },
  get_model_path: { domain: 'model', command: 'get_model_path' },
  delete_model: { domain: 'model', command: 'delete_model' },
  list_models: { domain: 'model', command: 'list_models' },
  get_model_download_path: { domain: 'model', command: 'get_model_download_path' },

  // File operations - additional
  rename_document: { domain: 'file', command: 'rename_document' },

  // Cache management
  clear_search_cache: { domain: 'cache', command: 'clear_search_cache' },
  clear_cache: { domain: 'cache', command: 'clear_cache' },
  get_cache_stats: { domain: 'cache', command: 'get_cache_stats' },
  get_cache_metrics: { domain: 'cache', command: 'get_cache_metrics' },
  get_llm_cache_stats: { domain: 'cache', command: 'get_llm_cache_stats' },

  // Additional commands for remaining APIs
  add_favorite: { domain: 'favorites', command: 'add_favorite' },
  remove_favorite: { domain: 'favorites', command: 'remove_favorite' },
  get_favorites: { domain: 'favorites', command: 'get_favorites' },
  is_favorite: { domain: 'favorites', command: 'is_favorite' },
  track_document_access: { domain: 'recent', command: 'track_document_access' },
  clear_recent_documents: { domain: 'recent', command: 'clear_recent_documents' },
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
  initialize_models: { domain: 'embeddings', command: 'initialize_models' },
  parse_wikilinks: { domain: 'extraction', command: 'parse_wikilinks' },
  extract_document_title: { domain: 'extraction', command: 'extract_document_title' },
  resolve_wikilink: { domain: 'extraction', command: 'resolve_wikilink' },
  extract_and_resolve_links: { domain: 'extraction', command: 'extract_and_resolve_links' },
  generate_tags_llm: { domain: 'text', command: 'generate_tags_llm' },
  generate_tags_batch: { domain: 'text', command: 'generate_tags_batch' },
  get_settings: { domain: 'settings', command: 'get_settings' },
  get_settings_category: { domain: 'settings', command: 'get_settings_category' },
  update_settings: { domain: 'settings', command: 'update_settings' },
  reset_settings: { domain: 'settings', command: 'reset_settings' },
  export_settings: { domain: 'settings', command: 'export_settings' },
  import_settings: { domain: 'settings', command: 'import_settings' },
  get_system_theme: { domain: 'settings', command: 'get_system_theme' },
  validate_folder_path: { domain: 'settings', command: 'validate_folder_path' },
  test_ollama_connection: { domain: 'settings', command: 'test_ollama_connection' },
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
  get_watch_folders: { domain: 'config', command: 'get_watch_folders' },
  add_watch_folder: { domain: 'config', command: 'add_watch_folder' },
  remove_watch_folder: { domain: 'config', command: 'remove_watch_folder' },
  ingest_web_url: { domain: 'web', command: 'ingest_web_url' },
  fetch_url_preview: { domain: 'web', command: 'fetch_url_preview' },
  extract_article: { domain: 'web', command: 'extract_article' },
  reindex_web_archive: { domain: 'web', command: 'reindex_web_archive' },
  migrate_web_archive: { domain: 'web', command: 'migrate_web_archive' },
  ingest_web_urls_batch: { domain: 'web', command: 'ingest_web_urls_batch' },
  cancel_batch_ingest: { domain: 'web', command: 'cancel_batch_ingest' },
  check_for_updates: { domain: 'updates', command: 'check_for_updates' },
  execute_function: { domain: 'functions', command: 'execute_function' },
  list_available_functions: { domain: 'functions', command: 'list_available_functions' },
  get_function_stats: { domain: 'functions', command: 'get_function_stats' },
  create_backup: { domain: 'backup', command: 'create_backup' },
  list_backups: { domain: 'backup', command: 'list_backups' },
  restore_backup: { domain: 'backup', command: 'restore_backup' },
  export_markdown: { domain: 'backup', command: 'export_markdown' },
  export_json: { domain: 'backup', command: 'export_json' },
  export_csv: { domain: 'backup', command: 'export_csv' },
  export_html: { domain: 'backup', command: 'export_html' },
  import_obsidian_vault: { domain: 'backup', command: 'import_obsidian_vault' },
  import_notion_export: { domain: 'backup', command: 'import_notion_export' },
  import_roam_json: { domain: 'backup', command: 'import_roam_json' },
  start_auto_backup: { domain: 'backup', command: 'start_auto_backup' },
  stop_auto_backup: { domain: 'backup', command: 'stop_auto_backup' },
  get_metrics: { domain: 'metrics', command: 'get_metrics' },
  get_version_info: { domain: 'updates', command: 'get_version_info' },
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
      console.log(`[API] Invoke attempt ${idx + 1}/${attempts.length}: ${signature}`, args);
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
  try {
    // Check if command should be routed through plugin system
    const pluginRoute = COMMAND_DOMAIN_MAP[command];

    let data: T;
    if (pluginRoute) {
      const rawResult = await invokeCommandWithFallback<T>(
        command,
        args,
        pluginRoute.domain,
        pluginRoute.command
      );
      console.log(`[API] Plugin result for ${command}:`, typeof rawResult, rawResult);
      data = rawResult;
    } else {
      // Legacy direct command (for unmapped commands)
      console.warn(`[API] Command '${command}' not in domain map, using legacy invoke`);
      data = await invoke<T>(command, args);
    }

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

function computeBatchProgress(totalItems: number, completedItems: number, failedItems: number): number {
  if (totalItems <= 0) {
    return 0;
  }
  const processed = completedItems + failedItems;
  return Math.min(100, Math.max(0, (processed / totalItems) * 100));
}

function normalizeBatchSummary(raw: BatchJobSummary): BatchJobSummary {
  const jobId = raw.jobId || raw.id || (raw as { job_id?: string }).job_id || '';
  const totalItems =
    raw.totalItems ?? (raw as { total_items?: number }).total_items ?? 0;
  const completedItems =
    raw.completedItems ?? (raw as { completed_items?: number }).completed_items ?? 0;
  const failedItems =
    raw.failedItems ?? (raw as { failed_items?: number }).failed_items ?? 0;
  const progress =
    raw.progress ?? computeBatchProgress(totalItems, completedItems, failedItems);

  return {
    ...raw,
    jobId,
    id: jobId,
    totalItems,
    completedItems,
    failedItems,
    progress,
  };
}

function normalizeBatchStatus(raw: BatchJobStatus): BatchJobStatus {
  const jobId = raw.jobId || raw.id || (raw as { job_id?: string }).job_id || '';
  const totalItems =
    raw.totalItems ?? raw.total_items ?? 0;
  const completedItems =
    raw.completedItems ?? raw.completed_items ?? 0;
  const failedItems =
    raw.failedItems ?? raw.failed_items ?? 0;
  const progress =
    raw.progress ?? computeBatchProgress(totalItems, completedItems, failedItems);

  return {
    ...raw,
    jobId,
    id: jobId,
    totalItems,
    completedItems,
    failedItems,
    progress,
    items: (raw.items || []).map((item) => ({
      ...item,
      itemId: item.itemId || item.id || (item as { item_id?: string }).item_id || '',
      target: item.target || item.url || (item as { target?: string }).target || '',
      id: item.itemId || item.id,
      url: item.target || item.url,
    })),
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
    return {
      ok: false,
      error: typeof nestedError.message === 'string' ? nestedError.message : fallbackError,
      details: nestedError as any,
    };
  }

  return { ok: true, data: response.data as T };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function normalizeSearchResult(raw: unknown): SearchResult | null {
  if (!isRecord(raw)) {
    return null;
  }

  const id = typeof raw.id === 'string' ? raw.id : '';
  const title = typeof raw.title === 'string' ? raw.title : id;
  const content = typeof raw.content === 'string' ? raw.content : '';
  const score = typeof raw.score === 'number' ? raw.score : 0;

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

function extractSearchResults(payload: unknown): SearchResult[] {
  if (Array.isArray(payload)) {
    return payload
      .map(normalizeSearchResult)
      .filter((item): item is SearchResult => item !== null);
  }

  if (!isRecord(payload)) {
    return [];
  }

  if (typeof payload.ok === 'boolean' && 'data' in payload) {
    return extractSearchResults(payload.data);
  }

  if (Array.isArray(payload.results)) {
    return extractSearchResults(payload.results);
  }

  if (Array.isArray(payload.data)) {
    return extractSearchResults(payload.data);
  }

  return [];
}

function unwrapSearchResults(
  response: ApiResult<unknown>,
  fallbackError: string
): ApiResult<SearchResult[]> {
  const unwrapped = unwrapNestedApiResult<unknown>(response, fallbackError);
  if (!unwrapped.ok) {
    return unwrapped;
  }

  return { ok: true, data: extractSearchResults(unwrapped.data) };
}

interface BackendIndexProgress {
  is_indexing: boolean;
  current_file: string | null;
  files_processed: number;
  total_files?: number | null;
  percent_complete?: number | null;
}

function normalizeIndexProgress(raw: BackendIndexProgress): IndexProgress {
  const totalFiles = raw.total_files ?? 0;
  const processed = raw.files_processed ?? 0;
  const percentage =
    raw.percent_complete ?? (totalFiles > 0 ? (processed / totalFiles) * 100 : 0);

  const status: IndexStatus =
    raw.is_indexing
      ? 'processing'
      : percentage >= 100 && totalFiles > 0
        ? 'complete'
        : 'idle';

  return {
    totalFiles,
    processed,
    failed: 0,
    currentFile: raw.current_file ?? undefined,
    status,
    percentage,
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
  // ============================================================
  // Database Management
  // ============================================================

  /**
   * Initializes the SQLite database with required schema and tables.
   * Creates all necessary tables for documents, embeddings, tags, and metadata.
   * Should be called once on application startup.
   *
   * @returns Success message confirming database initialization
   */
  initializeDatabase: async (): Promise<ApiResult<string>> => apiCall<string>('initialize_database'),

  // ============================================================
  // Search
  // ============================================================

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
  }): Promise<ApiResult<SearchResult[]>> =>
    apiCall<SearchResult[]>('search_with_recency', { options }),

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
  ): Promise<ApiResult<SearchResult[][]>> =>
    apiCall<SearchResult[][]>('batch_search', { queries, limit, search_mode: searchMode }),

  /**
   * Performs fast full-text search without semantic embeddings.
   * Uses SQLite FTS5 for keyword matching. Ideal for simple queries.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching documents ranked by relevance
   */
  searchFast: async (query: string, limit?: number): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_fast', { query, limit });
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
    const request = { query, limit: limit || 10 };
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
  findSimilar: async (chunkId: string, limit?: number): Promise<ApiResult<SearchResult[]>> => apiCall<SearchResult[]>('find_similar', { chunkId, limit }),


  // ============================================================
  // Indexing
  // ============================================================

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
      space_id: spaceId,
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
    apiCall<IndexFileResponse>('index_file', { path, space_id: spaceId, spaceId }),

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
  ): Promise<ApiResult<{ status: string; message: string; newName: string }>> => apiCall<{ status: string; message: string; newName: string }>(
      'rename_document',
      { documentId, newName }
    ),

  /**
   * Retrieves current indexing progress for active indexing operations.
   * Includes files processed, total files, current file, and completion percentage.
   *
   * @returns IndexProgress object with current state
   */
  getIndexProgress: async (): Promise<ApiResult<IndexProgress>> => {
    const result = await apiCall<BackendIndexProgress>('get_index_progress');
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

  /**
   * Gets comprehensive statistics about indexed content.
   * Includes total documents, total size, file type breakdown, and more.
   *
   * @returns IndexingStats object with counts and metrics
   */
  getIndexingStats: async (): Promise<ApiResult<IndexingStats>> => {
    const result = await apiCall<IndexingStats>('get_indexing_stats');
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
  getIndexedFolders: async (): Promise<ApiResult<IndexedFolder[]>> => apiCall<IndexedFolder[]>('get_indexed_folders'),

  /**
   * Removes an entire folder from the index.
   * Deletes all documents, embeddings, and metadata for files in that folder.
   *
   * @param path - Absolute path to folder to remove
   * @returns Void on success
   */
  removeIndexedFolder: async (path: string): Promise<ApiResult<void>> => apiCall<void>('remove_indexed_folder', { path }),

  /**
   * Retrieves recent indexing activity history.
   * Shows files indexed, errors, and timestamps for debugging and monitoring.
   *
   * @param limit - Maximum number of activity records to return
   * @returns Array of IndexingActivity objects ordered by recency
   */
  getIndexingActivities: async (limit: number): Promise<ApiResult<IndexingActivity[]>> => apiCall<IndexingActivity[]>('get_indexing_activities', { limit }),

  // ============================================================
  // File Management
  // ============================================================

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
  getRecentDocuments: async (limit: number): Promise<ApiResult<RecentDocument[]>> => apiCall<RecentDocument[]>('get_recent_documents', { limit }),

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
  listAllDocuments: async (limit: number): Promise<ApiResult<DocumentMetadata[]>> => apiCall<DocumentMetadata[]>('list_all_documents', { limit }),

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
  getFileMetadata: async (path: string): Promise<ApiResult<FileMetadata>> => apiCall<FileMetadata>('get_file_metadata', { path }),

  /**
   * Opens the file's parent folder in the system file manager.
   * Highlights the file if the platform supports it (e.g., Finder, Explorer).
   *
   * @param path - Absolute path to file
   * @returns Void on success
   */
  showInFolder: async (path: string): Promise<ApiResult<void>> => apiCall<void>('show_in_folder', { path }),

  // ============================================================
  // Configuration
  // ============================================================

  /**
   * Retrieves the current application configuration.
   * Includes all user preferences, paths, and feature flags.
   *
   * @returns AppConfig object with current settings
   */
  getConfig: async (): Promise<ApiResult<AppConfig>> => apiCall<AppConfig>('get_config'),

  /**
   * Saves application configuration to disk.
   * Persists user preferences and settings for next session.
   *
   * @param config - Complete AppConfig object to save
   * @returns Void on success
   */
  saveConfig: async (config: AppConfig): Promise<ApiResult<void>> => apiCall<void>('save_config', { config }),

  /**
   * Gets list of folders currently being watched for changes.
   * File watcher automatically reindexes modified files in these folders.
   *
   * @returns Array of absolute folder paths being monitored
   */
  getWatchFolders: async (): Promise<ApiResult<string[]>> => apiCall<string[]>('get_watch_folders'),

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

  // ============================================================
  // Web Ingest
  // ============================================================

  /**
   * Fetches preview metadata for a web URL without ingesting it.
   * Useful for validating and showing a preview before import.
   *
   * @param url - Web URL to preview (must be valid HTTP/HTTPS)
   * @returns UrlPreview metadata
   */
  fetchUrlPreview: async (url: string): Promise<ApiResult<UrlPreview>> =>
    apiCall<UrlPreview>('fetch_url_preview', { url }),

  /**
   * Extracts article content (reader mode) from a URL.
   * Returns clean text/HTML without ingesting.
   *
   * @param url - Web URL to extract
   * @returns CleanArticle content
   */
  extractArticle: async (url: string): Promise<ApiResult<CleanArticle>> =>
    apiCall<CleanArticle>('extract_article', { url }),

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
    apiCall<WebIngestResponse>('ingest_web_url', {
      url,
      space_id: options?.spaceId,
      spaceId: options?.spaceId,
      conversation_id: options?.conversationId,
      conversationId: options?.conversationId,
    }),

  /**
   * Ingest a batch of URLs in one Tauri call.
   *
   * Counts as ONE request against the `web_ingest` rate limiter (per-URL
   * pacing is internal — currently 250ms between URLs). Capped at 50 URLs
   * per call. Continues on per-URL failure and reports it in the summary.
   *
   * Subscribe to the `web-ingest:progress` Tauri event to update UI as
   * each URL completes; a `web-ingest:complete` event fires once at the
   * end (whether the batch finished or was cancelled).
   *
   * Refuses to start if another batch is already running for the same
   * `conversationId` (returns INVALID_STATE in that case).
   */
  ingestWebUrlsBatch: async (
    urls: string[],
    options?: { spaceId?: string; conversationId?: string }
  ): Promise<ApiResult<BatchIngestSummary>> =>
    apiCall<BatchIngestSummary>('ingest_web_urls_batch', {
      urls,
      space_id: options?.spaceId,
      spaceId: options?.spaceId,
      conversation_id: options?.conversationId,
      conversationId: options?.conversationId,
    }),

  /**
   * Cancel an in-flight web-ingest batch by `batchId`.
   *
   * Cancellation is cooperative — the in-flight URL completes, then the
   * loop exits and a `web-ingest:complete` event with `cancelled=true`
   * is emitted. Returns `true` if the batch existed and was flagged,
   * `false` if it had already finished.
   */
  cancelBatchIngest: async (batchId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('cancel_batch_ingest', {
      batch_id: batchId,
      batchId,
    }),

  /**
   * Reindexes all web archive files for searchability.
   * Scans the web archive directory (`~/.recall/web-archive/`) and reindexes all
   * markdown files found within. This ensures archived web content is searchable
   * and appears in FileBrowser.
   *
   * Useful for:
   * - Making archived content searchable after upgrading the app
   * - Recovering from indexing failures
   * - Rebuilding the index after database corruption
   *
   * @returns Number of files successfully reindexed
   *
   * @example
   * const result = await VaultAPI.reindexWebArchive();
   * if (result.ok) {
   *   console.log(`Reindexed ${result.data} web archive files`);
   * }
   */
  reindexWebArchive: async (): Promise<ApiResult<number>> => apiCall<number>('reindex_web_archive'),

  /**
   * Migrates legacy web archive files to new format.
   * Updates web archive storage structure for compatibility with latest version.
   *
   * @returns Number of files successfully migrated
   */
  migrateWebArchive: async (): Promise<ApiResult<number>> => apiCall<number>('migrate_web_archive'),

  // ============================================================
  // Daily Notes
  // ============================================================

  /**
   * Retrieves the daily note for today's date.
   * Creates a new daily note if one doesn't exist for today.
   *
   * @returns Daily note object with content and metadata
   */
  getTodayNote: async (): Promise<ApiResult<DailyNote>> => apiCall<DailyNote>('get_today_note'),

  /**
   * Quickly captures text to today's daily note.
   * Appends content to existing note or creates new note if needed.
   * Ideal for inbox-style quick capture workflows.
   *
   * @param content - Text content to append to today's note
   * @returns Void on success
   */
  quickCapture: async (content: string): Promise<ApiResult<void>> => apiCall<void>('quick_capture', { content }),

  /**
   * Retrieves all daily notes within a date range.
   * Useful for calendar views and date-based navigation.
   *
   * @param startDate - Start date in ISO format (YYYY-MM-DD)
   * @param endDate - End date in ISO format (YYYY-MM-DD)
   * @returns Array of daily note objects within the range
   */
  getDailyNotesRange: async (startDate: string, endDate: string): Promise<ApiResult<DailyNote[]>> =>
    apiCall<DailyNote[]>('get_daily_notes_range', { request: { startDate, endDate } }),

  /**
   * Gets the daily note immediately before the specified date.
   * Returns null if no earlier note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Previous daily note or null
   */
  getPreviousDailyNote: async (currentDate: string): Promise<ApiResult<DailyNote | null>> =>
    apiCall<DailyNote | null>('get_previous_daily_note', { request: { currentDate } }),

  /**
   * Gets the daily note immediately after the specified date.
   * Returns null if no later note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Next daily note or null
   */
  getNextDailyNote: async (currentDate: string): Promise<ApiResult<DailyNote | null>> =>
    apiCall<DailyNote | null>('get_next_daily_note', { request: { currentDate } }),

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
   * Lists all persisted notes in the Daily Notes workspace.
   */
  listWorkspaceNotes: async (): Promise<ApiResult<ListWorkspaceNotesResponse>> =>
    apiCall<ListWorkspaceNotesResponse>('list_workspace_notes'),

  /**
   * Creates a new persisted workspace note.
   */
  createWorkspaceNote: async (title?: string): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<WorkspaceNote>('create_workspace_note', { request: { title } }),

  /**
   * Persists a workspace note update.
   */
  updateWorkspaceNote: async (note: WorkspaceNote): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<WorkspaceNote>('update_workspace_note', { note }),

  /**
   * Deletes a persisted workspace note by id.
   */
  deleteWorkspaceNote: async (noteId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_workspace_note', { request: { noteId } }),

  // ============================================================
  // Settings
  // ============================================================

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
  updateSettings: async (settings: Record<string, unknown>): Promise<ApiResult<AppSettings>> =>
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

  /**
   * Tests Ollama connectivity and fetches models from the server.
   * Attempts OpenAI-compatible `/v1/models` first, then Ollama `/api/tags`.
   *
   * @param request - Connection details and optional auth header
   * @returns Endpoint used and available model IDs
   */
  testOllamaConnection: async (
    request: TestOllamaConnectionRequest
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<TestOllamaConnectionResponse>('test_ollama_connection', { request }),

  /**
   * Tests a custom tool endpoint with a sample query.
   * Useful for validating endpoint/query-param wiring before saving.
   */
  testCustomTool: async (
    request: TestCustomToolRequest
  ): Promise<ApiResult<TestCustomToolResponse>> =>
    apiCall<TestCustomToolResponse>('test_custom_tool', { request }),

  // ============================================================
  // Tag Management (DDD Architecture)
  // ============================================================

  /**
   * Creates a new tag with name and optional color.
   * Tags are used to categorize and organize documents.
   *
   * @param request - Tag creation request with name (required) and color (optional)
   * @returns Created tag with ID and metadata
   *
   * @example
   * const result = await VaultAPI.createTag({
   *   name: 'Research',
   *   color: '#FF5733'
   * });
   * if (result.ok) {
   *   console.log(`Created: ${result.data.tag.name}`);
   * }
   */
  createTag: async (request: CreateTagRequest): Promise<ApiResult<CreateTagResponse>> =>
    apiCall<CreateTagResponse>('create_tag', { request }),

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
  listAllTags: async (): Promise<ApiResult<ListTagsResponse>> =>
    apiCall<ListTagsResponse>('get_all_tags_with_counts'),

  /**
   * Renames an existing tag while preserving all document associations.
   *
   * @param request - Rename request with tag ID and new name
   * @returns Updated tag object
   *
   * @example
   * const result = await VaultAPI.renameTag({
   *   tagId: 'tag_123',
   *   newName: 'New Research'
   * });
   */
  renameTag: async (request: RenameTagRequest): Promise<ApiResult<RenameTagResponse>> =>
    apiCall<RenameTagResponse>('rename_tag', { request }),

  /**
   * Sets the color of an existing tag.
   *
   * @param request - Set color request with tag ID and color hex code
   * @returns Updated tag object
   *
   * @example
   * const result = await VaultAPI.setTagColor({
   *   tagId: 'tag_123',
   *   color: '#00FF00'
   * });
   */
  setTagColor: async (request: SetTagColorRequest): Promise<ApiResult<SetTagColorResponse>> =>
    apiCall<SetTagColorResponse>('set_tag_color', { request }),

  /**
   * Deletes a tag and removes all document associations.
   * Irreversible operation.
   *
   * @param request - Delete request with tag ID
   * @returns Void on success
   *
   * @example
   * const result = await VaultAPI.deleteTag({ tagId: 'tag_123' });
   */
  deleteTag: async (request: DeleteTagRequest): Promise<ApiResult<void>> =>
    apiCall<void>('delete_tag', { request }),

  /**
   * Adds a tag to a document.
   * Creates the association between tag and document.
   *
   * @param request - Request with document ID and tag ID
   * @returns Void on success
   *
   * @example
   * const result = await VaultAPI.addTagToDocument({
   *   documentId: 'doc_123',
   *   tagId: 'tag_456'
   * });
   */
  addTagToDocument: async (request: AddTagToDocumentRequest): Promise<ApiResult<void>> =>
    apiCall<void>('add_tag_to_document', { request }),

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
    apiCall<void>('remove_tag_from_document', { request }),

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
  getDocumentTags: async (request: GetDocumentTagsRequest): Promise<ApiResult<DocumentTagsResponse>> =>
    apiCall<DocumentTagsResponse>('get_document_tags', { documentId: request.documentId }),

  /**
   * Searches for tags by query string.
   * Performs fuzzy search on tag names.
   *
   * @param request - Search request with query string
   * @returns Matching tags with count
   *
   * @example
   * const result = await VaultAPI.searchTags({ query: 'research' });
   * if (result.ok) {
   *   console.log(`Found ${result.data.count} matching tags`);
   * }
   */
  searchTags: async (request: SearchTagsRequest): Promise<ApiResult<SearchTagsResponse>> =>
    apiCall<SearchTagsResponse>('search_tags', { request }),

  /**
   * Gets statistics for a tag including document count and recent documents.
   * Returns usage metrics and recent documents.
   *
   * @param request - Request with tag ID
   * @returns Tag statistics with document count and recent docs
   *
   * @example
   * const result = await VaultAPI.getTagStats({ tagId: 'tag_123' });
   * if (result.ok) {
   *   console.log(`Tag has ${result.data.documentCount} documents`);
   * }
   */
  getTagStats: async (request: GetTagStatsRequest): Promise<ApiResult<TagStatsResponse>> =>
    apiCall<TagStatsResponse>('get_tag_stats', { request }),

  /**
   * Finds all documents that have a specific tag.
   * Returns document IDs for all documents with this tag.
   *
   * @param request - Request with tag name
   * @returns Response with document IDs and count
   *
   * @example
   * const result = await VaultAPI.findDocumentsByTag({ tagName: 'Research' });
   * if (result.ok) {
   *   console.log(`Found ${result.data.count} documents`);
   * }
   */
  findDocumentsByTag: async (request: FindDocumentsByTagRequest): Promise<ApiResult<TaggedDocumentsResponse>> =>
    apiCall<TaggedDocumentsResponse>('find_documents_by_tag', { request }),

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
  getAllTagsWithCounts: async (): Promise<ApiResult<TagWithCount[]>> =>
    apiCall<TagWithCount[]>('get_all_tags_with_counts'),

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
    apiCall<Tag[]>('apply_tags', {
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

  // ============================================================
  // Q&A (Question Answering)
  // ============================================================

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
    const request = { question, contextLimit };
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
    const request = { question, contextLimit };
    return apiCall<QAResponse>('ask_question_stream', { request });
  },

  /**
   * Checks if the LLM (Large Language Model) service is healthy and accessible.
   * Verifies API key, endpoint connectivity, and model availability.
   *
   * @returns Health status object with connectivity and model info
   */
  checkLLMHealth: async (): Promise<ApiResult<LLMHealthStatus>> => apiCall<LLMHealthStatus>('check_llm_health'),

  // ============================================================
  // Favorites
  // ============================================================

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
  getFavorites: async (): Promise<ApiResult<FavoriteDocument[]>> => apiCall<FavoriteDocument[]>('get_favorites'),

  /**
   * Checks if a document is marked as favorite.
   *
   * @param documentId - Internal document identifier
   * @returns True if document is favorited, false otherwise
   */
  isFavorite: async (documentId: string): Promise<ApiResult<boolean>> => apiCall<boolean>('is_favorite', { documentId }),

  // ============================================================
  // Recent Documents
  // ============================================================

  /**
   * Records a document access for recency tracking.
   * Updates the document's last accessed timestamp.
   *
   * @param documentId - Internal document identifier
   * @returns Void on success
   */
  trackDocumentAccess: async (documentId: string): Promise<ApiResult<void>> => apiCall<void>('track_document_access', { documentId }),

  /**
   * Clears the recent documents history.
   * Removes all access timestamps but keeps documents indexed.
   *
   * @returns Void on success
   */
  clearRecentDocuments: async (): Promise<ApiResult<void>> => apiCall<void>('clear_recent_documents'),

  // ============================================================
  // Credentials
  // ============================================================

  /**
   * Stores an API key securely for a service.
   * Uses platform keychain/credential manager for secure storage.
   *
   * @param service - Service identifier (e.g., 'openai', 'anthropic')
   * @param apiKey - API key to store securely
   * @returns Void on success
   */
  setApiKey: async (service: string, apiKey: string): Promise<ApiResult<void>> => apiCall<void>('set_api_key', { service, apiKey }),

  /**
   * Retrieves a stored API key for a service.
   * Reads from secure platform credential storage.
   *
   * @param service - Service identifier
   * @returns API key string
   */
  getApiKey: async (service: string): Promise<ApiResult<string>> => apiCall<string>('get_api_key', { service }),

  /**
   * Deletes a stored API key for a service.
   * Removes credential from secure storage.
   *
   * @param service - Service identifier
   * @returns Void on success
   */
  deleteApiKey: async (service: string): Promise<ApiResult<void>> => apiCall<void>('delete_api_key', { service }),

  /**
   * Checks if an API key is stored for a service.
   *
   * @param service - Service identifier
   * @returns True if API key exists, false otherwise
   */
  hasApiKey: async (service: string): Promise<ApiResult<boolean>> => apiCall<boolean>('has_api_key', { service }),

  /**
   * Clears all stored credentials from secure storage.
   * Removes API keys for all services.
   *
   * @returns Void on success
   */
  clearAllCredentials: async (): Promise<ApiResult<void>> => apiCall<void>('clear_all_credentials'),

  /**
   * Sets a custom API endpoint URL for a service.
   * Useful for self-hosted models or alternative providers.
   *
   * @param endpoint - Custom endpoint URL (e.g., 'http://localhost:11434')
   * @returns Void on success
   */
  setCustomEndpoint: async (endpoint: string): Promise<ApiResult<void>> => apiCall<void>('set_custom_endpoint', { endpoint }),

  /**
   * Retrieves a custom endpoint URL for a service.
   *
   * @returns Custom endpoint URL or null if not set
   */
  getCustomEndpoint: async (): Promise<ApiResult<string | null>> => apiCall<string | null>('get_custom_endpoint'),

  // ============================================================
  // Cache Management
  // ============================================================

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
  getCacheStats: async (): Promise<ApiResult<CacheStats>> => apiCall<CacheStats>('get_cache_stats'),

  /**
   * Gets detailed cache performance metrics.
   * Returns metrics for monitoring and optimization.
   *
   * @returns Cache metrics object with detailed statistics
   */
  getCacheMetrics: async (): Promise<ApiResult<CacheMetrics>> => apiCall<CacheMetrics>('get_cache_metrics'),

  // ============================================================
  // Health & Metrics
  // ============================================================

  /**
   * Performs a health check on the backend system (DDD architecture).
   * Verifies database connectivity, model availability, and system status.
   *
   * @returns Health status object with component statuses
   * @note Updated to use DDD architecture command (Phase 1 migration)
   */
  healthCheck: async (): Promise<ApiResult<HealthStatus>> => apiCall<HealthStatus>('health_check'),

  /**
   * Retrieves system statistics (DDD architecture).
   * Includes total documents, chunks, tags, and storage size.
   *
   * @returns System statistics object
   * @note Updated to use DDD architecture command (Phase 1 migration)
   */
  getSystemStats: async (): Promise<ApiResult<SystemStats>> => apiCall<SystemStats>('get_system_stats'),

  /**
   * Retrieves system performance metrics (legacy).
   * Includes query counts, response times, and resource usage.
   *
   * @returns Metrics object with performance data
   * @deprecated Use getSystemStats for DDD architecture
   */
  getMetrics: async (): Promise<ApiResult<MetricsSnapshot>> => apiCall<MetricsSnapshot>('get_metrics'),

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
  getVersionInfo: async (): Promise<ApiResult<Record<string, unknown>>> => apiCall<Record<string, unknown>>('get_version_info'),

  // ============================================================
  // Initialization Commands
  // ============================================================

  /**
   * Initializes embedding models for semantic search.
   * @deprecated Models are now auto-initialized at startup. This exists for compatibility.
   * @returns Success message
   */
  initializeModels: async (): Promise<ApiResult<string>> => apiCall<string>('initialize_models'),

  // ============================================================
  // HuggingFace Credentials
  // ============================================================

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
    apiCall<HfTokenStatus>('get_huggingface_token_status'),

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

  // ============================================================
  // Batch Job Management
  // ============================================================

  /**
   * Lists all batch jobs with pagination support.
   * Returns batch job history ordered by creation date (newest first).
   *
   * @param limit - Max jobs to return (default: 20)
   * @param offset - Skip N jobs for pagination (default: 0)
   * @returns Array of batch job summaries
   */
  listBatchJobs: async (limit?: number, offset?: number): Promise<ApiResult<BatchJobSummary[]>> => {
    const result = await apiCall<ListBatchJobsResponse>('list_batch_jobs', { limit, offset });
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
    const result = await apiCall<DeleteBatchJobResponse>('delete_batch_job', { jobId });
    if (!result.ok) {
      return result;
    }
    if (!result.data.success) {
      return { ok: false, error: 'Failed to delete batch job' };
    }
    return { ok: true, data: undefined };
  },

  /**
   * Retries failed items from a batch job by creating a new job.
   * Extracts failed URLs and creates a new batch import job.
   *
   * @param jobId - ID of job with failed items
   * @returns New job ID for the retry operation
   */
  retryFailedBatchItems: async (jobId: string): Promise<ApiResult<string>> => {
    const result = await apiCall<RetryFailedItemsResponse>('retry_failed_items', { jobId });
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
    const result = await apiCall<BatchJobStatus>('get_batch_job_status', { jobId });
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
    const result = await apiCall<CancelBatchJobResponse>('cancel_batch_job', { jobId });
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
  startBatchFileImport: async (filePaths: string[]): Promise<ApiResult<string>> =>
    apiCall<string>('start_batch_file_import', { filePaths }),

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
  ): Promise<ApiResult<string>> =>
    apiCall<string>('start_batch_url_import', {
      urls,
      extract_article: options?.extractArticle,
    }),

  // ============================================================
  // Mentions (Wikilinks & References)
  // ============================================================

  /**
   * Extracts mentions (wikilinks, @-mentions) from document content.
   * Parses syntax like [[page]] and @person to create bidirectional links.
   *
   * @param documentId - Internal document identifier
   * @param content - Document content to parse
   * @returns Array of extracted mention objects
   */
  extractMentions: async (documentId: string, content: string): Promise<ApiResult<ExtractMentionsResponse>> => apiCall<ExtractMentionsResponse>('extract_mentions', { documentId, content }),

  /**
   * Searches for mentions matching a query.
   * Finds mentions by name or partial name match.
   *
   * @param query - Search query for mention names
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching mention objects
   */
  searchMentions: async (query: string, limit?: number): Promise<ApiResult<SearchMentionsResponse>> => apiCall<SearchMentionsResponse>('search_mentions', { query, limit }),

  /**
   * Gets all mentions found in a specific document.
   * Returns outgoing links from this document to other entities.
   *
   * @param documentId - Internal document identifier
   * @returns Array of mention objects found in the document
   */
  getMentionsForDocument: async (documentId: string): Promise<ApiResult<MentionWithContext[]>> => apiCall<MentionWithContext[]>('get_mentions_for_document', { documentId }),

  /**
   * Gets backlinks for a mention (documents that reference it).
   * Returns all documents that mention this entity.
   *
   * @param mentionName - Name of the mention to find backlinks for
   * @returns Array of documents that reference this mention
   */
  getBacklinksForMention: async (mentionName: string): Promise<ApiResult<BacklinksResponse>> => apiCall<BacklinksResponse>('get_backlinks_for_mention', { mentionName }),

  /**
   * Gets mentions filtered by type.
   * Types include 'wikilink', 'person', 'hashtag', etc.
   *
   * @param mentionType - Type of mentions to retrieve
   * @returns Array of mentions of the specified type
   */
  getMentionsByType: async (mentionType: string): Promise<ApiResult<Mention[]>> => apiCall<Mention[]>('get_mentions_by_type', { mentionType }),

  /**
   * Creates a new mention entity.
   * Manually adds a mention that can be referenced in documents.
   *
   * @param name - Mention name/identifier
   * @param mentionType - Type of mention (e.g., 'person', 'concept')
   * @returns Created mention object
   */
  createMention: async (name: string, mentionType: string): Promise<ApiResult<Mention>> => apiCall<Mention>('create_mention', { name, mentionType }),

  /**
   * Deletes a mention entity.
   * Removes mention and all its backlink relationships.
   *
   * @param mentionId - Internal mention identifier
   * @returns Void on success
   */
  deleteMention: async (mentionId: string): Promise<ApiResult<void>> => apiCall<void>('delete_mention', { mentionId }),

  // ============================================================
  // Updates
  // ============================================================

  /**
   * Checks for application updates.
   * Queries the update server for newer versions.
   *
   * @returns Update information with available version and download URL
   */
  checkForUpdates: async (): Promise<ApiResult<UpdateInfo>> => apiCall<UpdateInfo>('check_for_updates'),

  // ============================================================
  // Embeddings
  // ============================================================

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
  getEmbeddingModelInfo: async (): Promise<ApiResult<EmbeddingModelInfo>> => apiCall<EmbeddingModelInfo>('get_embedding_model_info'),

  // ============================================================
  // Text Extraction & Parsing
  // ============================================================

  /**
   * Parses wikilinks from markdown text.
   * Extracts [[wikilink]] syntax and returns structured link objects.
   *
   * @param text - Markdown text to parse
   * @param sourcePath - Source file path for relative link resolution (optional)
   * @returns Array of parsed wikilink objects
   */
  parseWikilinks: async (text: string, sourcePath?: string): Promise<ApiResult<ParsedLinksResponse>> => apiCall<ParsedLinksResponse>('parse_wikilinks', { text, sourcePath }),

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
   * @returns Resolved absolute file path
   */
  resolveWikilink: async (target: string, sourcePath: string): Promise<ApiResult<string>> => apiCall<string>('resolve_wikilink', { target, sourcePath }),

  /**
   * Extracts and resolves all links from document content.
   * Combines parsing and resolution into single operation.
   *
   * @param content - Document content to parse
   * @param sourcePath - Source file path for resolution
   * @returns Object with extracted and resolved links
   */
  extractAndResolveLinks: async (content: string, sourcePath: string): Promise<ApiResult<ExtractAndResolveLinksResponse>> => apiCall<ExtractAndResolveLinksResponse>('extract_and_resolve_links', { content, sourcePath }),

  /**
   * Generates tags for content using LLM (Large Language Model).
   * Analyzes content and suggests semantically relevant tags.
   *
   * @param content - Document content to analyze
   * @param maxTags - Maximum number of tags to generate (optional)
   * @returns Array of generated tag names
   */
  generateTagsLLM: async (content: string, maxTags?: number): Promise<ApiResult<string[]>> => apiCall<string[]>('generate_tags_llm', { content, maxTags }),

  /**
   * Generates tags for multiple documents in batch.
   * More efficient than calling generateTagsLLM repeatedly.
   *
   * @param documents - Array of document objects with id and content
   * @param maxTags - Maximum tags per document (optional)
   * @returns Array of tag generation results per document
   */
  generateTagsBatch: async (
    documents: Array<{ id: string; content: string }>,
    maxTags?: number
  ): Promise<ApiResult<GenerateTagsBatchResponse>> => apiCall<GenerateTagsBatchResponse>('generate_tags_batch', { documents, maxTags }),

  /**
   * Gets cache statistics for LLM API calls.
   * Shows prompt caching efficiency and cost savings.
   *
   * @param apiKey - API key to check stats for (optional)
   * @returns LLM cache statistics object
   */
  getLLMCacheStats: async (apiKey?: string): Promise<ApiResult<LLMCacheStats>> => apiCall<LLMCacheStats>('get_llm_cache_stats', { apiKey }),

  // ============================================================
  // Conversation Operations (Wave 2B)
  // ============================================================

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
    apiCall<CreateConversationResponse>('create_conversation', {
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
    apiCall<ListConversationsResponse>('list_conversations', {
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
    apiCall<GetConversationResponse>('get_conversation', {
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
    apiCall<GetConversationMessagesResponse>('get_conversation_messages', {
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
    apiCall<RenameConversationResponse>('rename_conversation', {
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
    toolPreferences?: ToolPreferences
  ): Promise<ApiResult<{
    conversationId: string;
    messages: ConversationMessage[];
    contextUsed: number;
  }>> => {
    const payload: Record<string, unknown> = {
      // Send both naming styles for compatibility with Tauri arg deserialization.
      conversation_id: conversationId,
      conversationId,
      message,
    };

    if (toolPreferences) {
      payload.tool_preferences = toolPreferences;
      payload.toolPreferences = toolPreferences;
    }

    return apiCall<{
      conversationId: string;
      messages: ConversationMessage[];
      contextUsed: number;
    }>('chat_with_conversation', payload);
  },

  /**
   * Cancels active model generation for a conversation if one is currently in-flight.
   */
  cancelConversationGeneration: async (
    conversationId: string
  ): Promise<ApiResult<void>> => {
    const response = await apiCall<{
      conversationId: string;
      messages: ConversationMessage[];
      contextUsed: number;
    }>('chat_with_conversation', {
      conversation_id: conversationId,
      conversationId,
      message: '',
      cancel_only: true,
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
    apiCall<DeleteConversationResponse>('delete_conversation', {
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
    apiCall<ConversationSpaceDto>('create_conversation_space', { request }),

  /**
   * Lists all conversation spaces.
   */
  listConversationSpaces: async (): Promise<ApiResult<ConversationSpaceDto[]>> =>
    apiCall<ConversationSpaceDto[]>('list_conversation_spaces'),

  /**
   * Creates a journal notebook.
   */
  createJournal: async (
    request: CreateConversationJournalRequest
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<ConversationJournalDto>('create_journal', { request }),

  /**
   * Lists all journals.
   */
  listJournals: async (): Promise<ApiResult<ConversationJournalDto[]>> =>
    apiCall<ConversationJournalDto[]>('list_journals'),

  /**
   * Lists members for a specific conversation space.
   */
  listConversationSpaceMembers: async (
    spaceId: string
  ): Promise<ApiResult<ConversationSpaceMemberDto[]>> =>
    apiCall<ConversationSpaceMemberDto[]>('list_conversation_space_members', {
      space_id: spaceId,
      spaceId,
    }),

  /**
   * Adds/updates a conversation space member and role.
   */
  upsertConversationSpaceMember: async (
    request: UpsertConversationSpaceMemberRequest
  ): Promise<ApiResult<ConversationSpaceMemberDto>> =>
    apiCall<ConversationSpaceMemberDto>('upsert_conversation_space_member', { request }),

  /**
   * Removes a member from a conversation space.
   */
  removeConversationSpaceMember: async (
    request: RemoveConversationSpaceMemberRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('remove_conversation_space_member', { request }),

  /**
   * Updates a conversation space.
   */
  updateConversationSpace: async (
    request: UpdateConversationSpaceRequest
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<ConversationSpaceDto>('update_conversation_space', { request }),

  /**
   * Archives/unarchives a conversation space.
   */
  archiveConversationSpace: async (
    request: ArchiveConversationSpaceRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('archive_conversation_space', { request }),

  createConversationThread: async (
    request: CreateConversationThreadRequest
  ): Promise<ApiResult<ConversationThreadDto>> =>
    apiCall<ConversationThreadDto>('create_conversation_thread', { request }),

  listConversationThreads: async (
    request: ListConversationThreadsRequest
  ): Promise<ApiResult<ConversationThreadDto[]>> =>
    apiCall<ConversationThreadDto[]>('list_conversation_threads', { request }),

  updateConversationThread: async (
    request: UpdateConversationThreadRequest
  ): Promise<ApiResult<ConversationThreadDto>> =>
    apiCall<ConversationThreadDto>('update_conversation_thread', { request }),

  archiveConversationThread: async (
    request: ArchiveConversationThreadRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('archive_conversation_thread', { request }),

  deleteConversationThread: async (
    request: DeleteConversationThreadRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('delete_conversation_thread', { request }),

  moveConversationToThread: async (
    request: MoveConversationToThreadRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('move_conversation_to_thread', { request }),

  /**
   * Updates a journal notebook.
   */
  updateJournal: async (
    request: UpdateConversationJournalRequest
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<ConversationJournalDto>('update_journal', { request }),

  /**
   * Archives/unarchives a journal notebook.
   */
  archiveJournal: async (
    request: ArchiveConversationJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('archive_journal', { request }),

  /**
   * Permanently deletes a journal notebook.
   */
  deleteJournal: async (
    request: DeleteConversationJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('delete_journal', { request }),

  /**
   * Moves a conversation to a space.
   */
  moveConversationToSpace: async (
    request: MoveConversationToSpaceRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('move_conversation_to_space', { request }),

  /**
   * Adds a conversation to a journal without changing its owning space.
   */
  addConversationToJournal: async (
    request: AddConversationToJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('add_conversation_to_journal', { request }),

  /**
   * Removes a conversation from a journal entry deck.
   */
  removeConversationFromJournal: async (
    request: RemoveConversationFromJournalRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('remove_conversation_from_journal', { request }),

  /**
   * Sets the "saved" state for a conversation.
   */
  setConversationSaved: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('set_conversation_saved', { request }),

  /**
   * Sets the "bookmarked" state for a conversation.
   */
  setConversationBookmarked: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('set_conversation_bookmarked', { request }),

  /**
   * Sets the "pinned" state for a conversation.
   */
  setConversationPinned: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('set_conversation_pinned', { request }),

  /**
   * Sets the "archived" state for a conversation.
   */
  setConversationArchived: async (
    request: SetConversationStateRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('set_conversation_archived', { request }),

  /**
   * Deletes a specific message from a conversation.
   */
  deleteConversationMessage: async (
    request: DeleteConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('delete_conversation_message', { request }),

  /**
   * Bookmarks a specific message in a conversation (upsert by message).
   */
  bookmarkConversationMessage: async (
    request: BookmarkConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('bookmark_conversation_message', { request }),

  /**
   * Removes a message bookmark from a conversation.
   */
  unbookmarkConversationMessage: async (
    request: UnbookmarkConversationMessageRequest
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('unbookmark_conversation_message', { request }),

  /**
   * Lists message-level bookmarks with optional conversation and text filters.
   */
  listMessageBookmarks: async (
    query?: ListMessageBookmarksQuery
  ): Promise<ApiResult<ListMessageBookmarksResponse>> =>
    apiCall<ListMessageBookmarksResponse>('list_message_bookmarks', {
      query: query ?? {},
    }),

  /**
   * Lists conversations for the explorer with filters and text search.
   */
  listConversationsExplorer: async (
    query?: ListConversationsExplorerQuery
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<ListConversationsResponse>('list_conversations_explorer', {
      query: query ?? {},
    }),

  /**
   * Lists conversations included in a journal (additive membership).
   */
  listJournalConversations: async (
    query: ListJournalConversationsQuery
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<ListConversationsResponse>('list_journal_conversations', {
      query,
    }),

  /**
   * Lists documents currently linked to a conversation context.
   */
  listConversationLinkedDocuments: async (
    conversationId: string
  ): Promise<ApiResult<ConversationLinkedDocumentDto[]>> =>
    apiCall<ConversationLinkedDocumentDto[]>('list_conversation_linked_documents', {
      conversation_id: conversationId,
      conversationId,
    }),

  /**
   * Removes a linked document reference from a conversation.
   */
  removeConversationLinkedDocument: async (
    conversationId: string,
    documentId: string
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('remove_conversation_linked_document', {
      conversation_id: conversationId,
      conversationId,
      document_id: documentId,
      documentId,
    }),

  /**
   * Adds a linked document reference to a conversation.
   */
  addConversationLinkedDocument: async (
    conversationId: string,
    documentId: string,
    options?: {
      chunkId?: string;
      relevanceScore?: number;
    }
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('add_conversation_linked_document', {
      conversation_id: conversationId,
      conversationId,
      document_id: documentId,
      documentId,
      chunk_id: options?.chunkId,
      chunkId: options?.chunkId,
      relevance_score: options?.relevanceScore,
      relevanceScore: options?.relevanceScore,
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
    apiCall<RenameConversationResponse>('add_conversation_web_source', {
      conversation_id: conversationId,
      conversationId,
      url,
      title: options?.title,
      excerpt: options?.excerpt,
      relevance_score: options?.relevanceScore,
      relevanceScore: options?.relevanceScore,
    }),

  /**
   * Lists non-ingested web sources currently linked to a conversation context.
   */
  listConversationWebSources: async (
    conversationId: string
  ): Promise<ApiResult<ConversationWebSourceDto[]>> =>
    apiCall<ConversationWebSourceDto[]>('list_conversation_web_sources', {
      conversation_id: conversationId,
      conversationId,
    }),

  /**
   * Removes a linked web source from a conversation context.
   */
  removeConversationWebSource: async (
    conversationId: string,
    sourceId: string
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('remove_conversation_web_source', {
      conversation_id: conversationId,
      conversationId,
      source_id: sourceId,
      sourceId,
    }),

  /**
   * Lists all spaces currently assigned to a document.
   */
  listDocumentSpaceMemberships: async (
    documentId: string
  ): Promise<ApiResult<DocumentSpaceMembershipDto[]>> =>
    apiCall<DocumentSpaceMembershipDto[]>('list_document_space_memberships', {
      document_id: documentId,
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
    apiCall<RenameConversationResponse>('set_document_space_membership', {
      document_id: documentId,
      documentId,
      space_id: spaceId,
      spaceId,
      assigned,
    }),

  /**
   * Synthesizes multiple conversations into a structured journal summary.
   */
  synthesizeJournalEntries: async (
    request: SynthesizeJournalEntriesRequest
  ): Promise<ApiResult<SynthesizeJournalEntriesResponse>> =>
    apiCall<SynthesizeJournalEntriesResponse>('synthesize_journal_entries', {
      request,
    }),

  /**
   * Assigns or removes multiple documents from a space scope.
   */
  setDocumentsSpaceMembership: async (
    documentIds: string[],
    spaceId: string,
    assigned: boolean
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<RenameConversationResponse>('set_documents_space_membership', {
      document_ids: documentIds,
      documentIds,
      space_id: spaceId,
      spaceId,
      assigned,
    }),

  // ============================================================
  // Function Calling Operations (Wave 2B)
  // ============================================================

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
    args: Record<string, unknown>
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

  // ============================================================
  // Model Management - Downloads (Wave 2C)
  // ============================================================

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
    apiCall<DownloadStatus | null>('get_download_status', { id: downloadId }),

  /**
   * Lists all downloads (active, paused, completed, failed).
   * Returns comprehensive list for download management UI.
   *
   * @returns Array of all download statuses
   */
  listDownloads: async (): Promise<ApiResult<DownloadStatus[]>> =>
    apiCall<DownloadStatus[]>('list_downloads'),

  // ============================================================
  // Model Management - Downloaded Models (Wave 2C)
  // ============================================================

  /**
   * Gets all downloaded models with metadata.
   * Includes both chat and embedding models from the downloaded_models table.
   *
   * NOTE: Gateway Pattern returns structured data directly (no JSON string parsing needed).
   *
   * @returns Array of downloaded model records
   */
  getDownloadedModels: async (): Promise<ApiResult<DownloadedModel[]>> =>
    apiCall<DownloadedModel[]>('get_models_with_metadata'),

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
   * Gets the currently active chat model.
   * Returns null if no model is set as active.
   *
   * @returns Active chat model or null
   */
  getActiveChatModel: async (): Promise<ApiResult<DownloadedModel | null>> =>
    apiCall<DownloadedModel | null>('get_active_chat_model'),

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
    apiCall<DownloadedModel | null>('get_active_embedding_model'),

  getActiveModels: async (): Promise<ApiResult<{
    chat_model: DownloadedModel | null;
    embedding_model: DownloadedModel | null;
  }>> =>
    apiCall<{
      chat_model: DownloadedModel | null;
      embedding_model: DownloadedModel | null;
    }>('get_active_models'),

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

  // ============================================================
  // Model Management - LLM & Catalog (Wave 4B)
  // ============================================================

  /**
   * Gets system hardware capabilities for model selection.
   * Detects GPU, RAM, CPU cores, and platform to recommend compatible models.
   *
   * @returns System capabilities including GPU availability and resources
   */
  getSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<SystemCapabilities>('detect_system_capabilities'),

  /**
   * Lists all available models from the catalog.
   * Returns curated models that can be downloaded and used.
   *
   * @returns Array of available models with metadata
   */
  getAvailableModels: async (): Promise<ApiResult<ModelInfo[]>> =>
    apiCall<ModelInfo[]>('get_available_models'),

  /**
   * Gets recommended models for the current system.
   * Filters models by hardware compatibility (RAM, GPU, etc.).
   *
   * @returns Array of recommended models that will run on this system
   */
  getRecommendedModels: async (): Promise<ApiResult<ModelInfo[]>> =>
    apiCall<ModelInfo[]>('get_recommended_models'),

  /**
   * Gets the best model for a specific task and system.
   * Considers task type, hardware capabilities, and performance trade-offs.
   *
   * @param task - Task type (e.g., "chat", "embedding", "code")
   * @returns Best model for the task or null if none suitable
   */
  getBestModel: async (task: string): Promise<ApiResult<ModelInfo | null>> =>
    apiCall<ModelInfo | null>('get_best_model', { task }),

  /**
   * Checks if a model is downloaded by querying the catalog.
   * This is the catalog-level check (use isModelDownloaded for database check).
   *
   * @param modelId - Model identifier to check
   * @returns True if model is downloaded
   */
  isModelDownloadedCatalog: async (modelId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_model_downloaded', { modelId }),

  /**
   * Gets the filesystem path where a model is or will be stored.
   * Returns the path even if model isn't downloaded yet.
   *
   * @param modelId - Model identifier
   * @returns Absolute filesystem path for the model
   */
  getModelPath: async (modelId: string): Promise<ApiResult<string>> =>
    apiCall<string>('get_model_path', { modelId }),

  /**
   * Downloads a model from the catalog.
   * Starts a download job that can be tracked via download management.
   *
   * @param modelId - Model identifier to download
   * @returns Download ID for tracking progress
   */
  downloadModel: async (modelId: string): Promise<ApiResult<DownloadModelResponse>> =>
    apiCall<DownloadModelResponse>('download_model', { modelId }),

  /**
   * Starts a batch file import operation for indexing multiple files
   * @param filePaths - Array of absolute file paths to index
   * @returns Operation ID for tracking progress
   */
  batchFileImport: async (filePaths: string[]): Promise<ApiResult<string>> =>
    apiCall<string>('start_batch_file_import', { filePaths }),

  /**
   * Deletes a model file from disk.
   * Removes the model file but may leave database record.
   *
   * @param modelId - Model identifier to delete
   * @returns Void on success
   */
  deleteModel: async (modelId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_model', { modelId }),

  /**
   * Lists all models (downloaded + available).
   * Returns comprehensive list for model selection UI.
   *
   * @returns Array of all models with download status
   */
  listModels: async (): Promise<ApiResult<ModelInfo[]>> =>
    apiCall<ModelInfo[]>('list_models'),

  /**
   * Gets the directory where models are downloaded.
   * Returns the base path for model storage.
   *
   * @returns Absolute path to model download directory
   */
  getModelDownloadPath: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_model_download_path'),

  // ============================================================
  // Model Catalog Management (Wave 4B)
  // ============================================================

  /**
   * Detects system capabilities for model compatibility.
   * Similar to getSystemCapabilities but may include more detailed info.
   *
   * @returns Detailed system capabilities
   */
  detectSystemCapabilities: async (): Promise<ApiResult<SystemCapabilities>> =>
    apiCall<SystemCapabilities>('detect_system_capabilities'),

  /**
   * Gets all models compatible with the current system.
   * Filters catalog by RAM, GPU, and other requirements.
   *
   * @returns Array of compatible models
   */
  getCompatibleModels: async (category?: string): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<ModelRecommendation[]>('get_compatible_models', category ? { category } : {}),

  /**
   * Gets all recommended models across all tasks.
   * Returns best options for chat, embedding, etc.
   *
   * @returns Array of recommended models by task type
   */
  getAllRecommendedModels: async (): Promise<ApiResult<ModelRecommendation[]>> =>
    apiCall<ModelRecommendation[]>('get_all_recommended_models'),

  /**
   * Searches the model catalog by name or description.
   * Supports fuzzy matching for model discovery.
   *
   * @param query - Search query string
   * @returns Array of matching models with relevance scores
   */
  searchModelCatalog: async (request: SearchModelCatalogRequest): Promise<ApiResult<ModelSearchResult[]>> =>
    apiCall<ModelSearchResult[]>('search_model_catalog', { request }),

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
  clearModelCatalogCache: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_model_catalog_cache'),

  /**
   * Gets statistics about the model catalog cache.
   * Returns cache usage metrics and effectiveness.
   *
   * @returns Cache statistics
   */
  getModelCatalogStats: async (): Promise<ApiResult<ModelCatalogCacheStats>> =>
    apiCall<ModelCatalogCacheStats>('get_model_catalog_stats'),

  // ============================================================
  // Model State Management (Wave 4B)
  // ============================================================

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

  // ============================================================
  // Auto-Backup Operations (Wave 4B)
  // ============================================================

  /**
   * Starts automatic periodic backups.
   * Schedules backups at specified interval (e.g., "daily", "weekly").
   *
   * @param schedule - Backup schedule ("hourly", "daily", "weekly")
   * @returns Void on success
   */
  startAutoBackup: async (schedule: string): Promise<ApiResult<void>> =>
    apiCall<void>('start_auto_backup', { schedule }),

  /**
   * Stops automatic periodic backups.
   * Cancels scheduled backup jobs.
   *
   * @returns Void on success
   */
  stopAutoBackup: async (): Promise<ApiResult<void>> =>
    apiCall<void>('stop_auto_backup'),

  // ============================================================
  // Function Calling Stats (Wave 4B)
  // ============================================================

  /**
   * Gets statistics about LLM function calling usage.
   * Returns execution counts, success rates, and timing info.
   *
   * @returns Function calling statistics
   */
  getFunctionStats: async (): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall<Record<string, unknown>>('get_function_stats'),

  // ============================================================
  // Backup and Export Operations (Wave 2D)
  // ============================================================

  createBackup: async (backupPath?: string): Promise<ApiResult<string>> => apiCall<string>('create_backup', { backup_path: backupPath }),
  listBackups: async (): Promise<ApiResult<BackupInfo[]>> => apiCall<BackupInfo[]>('list_backups'),
  restoreBackup: async (backupPath: string): Promise<ApiResult<void>> => apiCall<void>('restore_backup', { backup_path: backupPath }),
  exportMarkdown: async (outputDir: string): Promise<ApiResult<number>> => apiCall<number>('export_markdown', { output_dir: outputDir }),
  exportJson: async (outputPath: string, pretty: boolean = false): Promise<ApiResult<void>> => apiCall<void>('export_json', { output_path: outputPath, pretty }),
  exportCsv: async (outputPath: string): Promise<ApiResult<number>> => apiCall<number>('export_csv', { output_path: outputPath }),
  exportHtml: async (outputDir: string): Promise<ApiResult<number>> => apiCall<number>('export_html', { output_dir: outputDir }),
  importObsidianVault: async (vaultPath: string): Promise<ApiResult<number>> => apiCall<number>('import_obsidian_vault', { vault_path: vaultPath }),
  importNotionExport: async (exportPath: string): Promise<ApiResult<number>> => apiCall<number>('import_notion_export', { export_path: exportPath }),
  importRoamJson: async (jsonPath: string): Promise<ApiResult<number>> => apiCall<number>('import_roam_json', { json_path: jsonPath }),

};

export { VaultAPI };
export default VaultAPI;
