/**
 * Gateway Pattern - Command to Domain mapping
 * Maps old command names to their new domain + command structure
 */
export const COMMAND_DOMAIN_MAP: Record<
  string,
  { domain: string; command: string }
> = {
  // Model domain
  get_models_with_metadata: {
    domain: 'model',
    command: 'list_downloaded_models',
  },
  is_model_already_downloaded: {
    domain: 'model',
    command: 'is_model_already_downloaded',
  },
  set_active_chat_model: { domain: 'model', command: 'set_active_chat_model' },
  warm_up_active_chat_model: {
    domain: 'model',
    command: 'warm_up_active_chat_model',
  },
  warm_up_active_utility_model: {
    domain: 'model',
    command: 'warm_up_active_utility_model',
  },
  get_active_chat_model: { domain: 'model', command: 'get_active_chat_model' },
  get_active_models: { domain: 'model', command: 'get_active_models' },
  delete_downloaded_model_and_file: {
    domain: 'model',
    command: 'delete_model',
  },
  get_active_embedding_model: {
    domain: 'model',
    command: 'get_active_embedding_model',
  },
  set_active_embedding_model: {
    domain: 'model',
    command: 'set_active_embedding_model',
  },
  detect_system_capabilities: {
    domain: 'model',
    command: 'detect_system_capabilities',
  },
  get_all_recommended_models: {
    domain: 'model',
    command: 'get_all_recommended_models',
  },
  get_model_catalog_stats: {
    domain: 'model',
    command: 'get_model_catalog_stats',
  },
  get_model_download_path: {
    domain: 'model',
    command: 'get_model_download_path',
  },

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
  list_custom_collections: {
    domain: 'file',
    command: 'list_custom_collections',
  },
  import_legacy_custom_collections: {
    domain: 'file',
    command: 'import_legacy_custom_collections',
  },
  create_custom_collection: {
    domain: 'file',
    command: 'create_custom_collection',
  },
  rename_custom_collection: {
    domain: 'file',
    command: 'rename_custom_collection',
  },
  move_custom_collection: { domain: 'file', command: 'move_custom_collection' },
  delete_custom_collection: {
    domain: 'file',
    command: 'delete_custom_collection',
  },
  add_documents_to_custom_collection: {
    domain: 'file',
    command: 'add_documents_to_custom_collection',
  },
  remove_documents_from_custom_collection: {
    domain: 'file',
    command: 'remove_documents_from_custom_collection',
  },
  get_indexing_activities: {
    domain: 'file',
    command: 'get_indexing_activities',
  },

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
  get_all_tags_with_counts: {
    domain: 'tags',
    command: 'get_all_tags_with_counts',
  },
  remove_tag_from_document: {
    domain: 'tags',
    command: 'remove_tag_from_document',
  },
  get_document_tags: { domain: 'tags', command: 'get_document_tags' },
  apply_tags: { domain: 'tags', command: 'apply_tags' },
  generate_tags_for_document: {
    domain: 'tags',
    command: 'generate_tags_for_document',
  },

  // QA domain
  check_llm_health: { domain: 'qa', command: 'check_llm_health_wrapper' },
  generate_chat_starters: {
    domain: 'qa',
    command: 'generate_chat_starters_wrapper',
  },

  // Conversation domain
  create_conversation: {
    domain: 'conversation',
    command: 'create_conversation',
  },
  list_conversations: { domain: 'conversation', command: 'list_conversations' },
  get_conversation: { domain: 'conversation', command: 'get_conversation' },
  get_conversation_messages: {
    domain: 'conversation',
    command: 'get_conversation_messages',
  },
  rename_conversation: {
    domain: 'conversation',
    command: 'rename_conversation',
  },
  chat_with_conversation: {
    domain: 'conversation',
    command: 'chat_with_conversation',
  },
  delete_conversation: {
    domain: 'conversation',
    command: 'delete_conversation',
  },
  create_conversation_space: {
    domain: 'conversation',
    command: 'create_conversation_space',
  },
  list_conversation_spaces: {
    domain: 'conversation',
    command: 'list_conversation_spaces',
  },
  create_journal: { domain: 'conversation', command: 'create_journal' },
  list_journals: { domain: 'conversation', command: 'list_journals' },
  list_conversation_space_members: {
    domain: 'conversation',
    command: 'list_conversation_space_members',
  },
  upsert_conversation_space_member: {
    domain: 'conversation',
    command: 'upsert_conversation_space_member',
  },
  remove_conversation_space_member: {
    domain: 'conversation',
    command: 'remove_conversation_space_member',
  },
  update_conversation_space: {
    domain: 'conversation',
    command: 'update_conversation_space',
  },
  archive_conversation_space: {
    domain: 'conversation',
    command: 'archive_conversation_space',
  },
  update_journal: { domain: 'conversation', command: 'update_journal' },
  archive_journal: { domain: 'conversation', command: 'archive_journal' },
  delete_journal: { domain: 'conversation', command: 'delete_journal' },
  move_conversation_to_space: {
    domain: 'conversation',
    command: 'move_conversation_to_space',
  },
  add_conversation_to_journal: {
    domain: 'conversation',
    command: 'add_conversation_to_journal',
  },
  remove_conversation_from_journal: {
    domain: 'conversation',
    command: 'remove_conversation_from_journal',
  },
  set_conversation_saved: {
    domain: 'conversation',
    command: 'set_conversation_saved',
  },
  set_conversation_bookmarked: {
    domain: 'conversation',
    command: 'set_conversation_bookmarked',
  },
  set_conversation_pinned: {
    domain: 'conversation',
    command: 'set_conversation_pinned',
  },
  set_conversation_archived: {
    domain: 'conversation',
    command: 'set_conversation_archived',
  },
  delete_conversation_message: {
    domain: 'conversation',
    command: 'delete_conversation_message',
  },
  bookmark_conversation_message: {
    domain: 'conversation',
    command: 'bookmark_conversation_message',
  },
  unbookmark_conversation_message: {
    domain: 'conversation',
    command: 'unbookmark_conversation_message',
  },
  list_message_bookmarks: {
    domain: 'conversation',
    command: 'list_message_bookmarks',
  },
  list_conversations_explorer: {
    domain: 'conversation',
    command: 'list_conversations_explorer',
  },
  list_journal_conversations: {
    domain: 'conversation',
    command: 'list_journal_conversations',
  },
  list_space_documents: {
    domain: 'conversation',
    command: 'list_space_documents',
  },
  list_conversation_linked_documents: {
    domain: 'conversation',
    command: 'list_conversation_linked_documents',
  },
  remove_conversation_linked_document: {
    domain: 'conversation',
    command: 'remove_conversation_linked_document',
  },
  add_conversation_web_source: {
    domain: 'conversation',
    command: 'add_conversation_web_source',
  },
  list_conversation_web_sources: {
    domain: 'conversation',
    command: 'list_conversation_web_sources',
  },
  remove_conversation_web_source: {
    domain: 'conversation',
    command: 'remove_conversation_web_source',
  },
  list_document_space_memberships: {
    domain: 'conversation',
    command: 'list_document_space_memberships',
  },
  set_document_space_membership: {
    domain: 'conversation',
    command: 'set_document_space_membership',
  },
  set_documents_space_membership: {
    domain: 'conversation',
    command: 'set_documents_space_membership',
  },
  add_documents_to_library: {
    domain: 'conversation',
    command: 'add_documents_to_library',
  },
  synthesize_journal_entries: {
    domain: 'conversation',
    command: 'synthesize_journal_entries',
  },
  truncate_conversation_after: {
    domain: 'conversation',
    command: 'truncate_conversation_after',
  },
  fork_conversation: { domain: 'conversation', command: 'fork_conversation' },
  create_conversation_tangent: { domain: 'conversation', command: 'create_conversation_tangent' },
  list_conversation_tangents: { domain: 'conversation', command: 'list_conversation_tangents' },
  promote_conversation_tangent: { domain: 'conversation', command: 'promote_conversation_tangent' },
  continue_in_new_conversation: {
    domain: 'conversation',
    command: 'continue_in_new_conversation',
  },
  regenerate_response: {
    domain: 'conversation',
    command: 'regenerate_response',
  },
  compact_conversation: {
    domain: 'conversation',
    command: 'compact_conversation',
  },
  manage_knowledge: { domain: 'conversation', command: 'manage_knowledge' },
  get_conversation_memory: {
    domain: 'conversation',
    command: 'get_conversation_memory',
  },

  // Passage references
  create_passage_reference: {
    domain: 'references',
    command: 'create_passage_reference',
  },
  list_passage_references: {
    domain: 'references',
    command: 'list_passage_references',
  },
  update_passage_reference: {
    domain: 'references',
    command: 'update_passage_reference',
  },
  delete_passage_reference: {
    domain: 'references',
    command: 'delete_passage_reference',
  },

  // Explorer domain: a folder on disk, read live beside a chat
  explorer_resolve_root: {
    domain: 'explorer',
    command: 'explorer_resolve_root',
  },
  explorer_list_dir: { domain: 'explorer', command: 'explorer_list_dir' },
  explorer_read_file: { domain: 'explorer', command: 'explorer_read_file' },
  explorer_locate_file: { domain: 'explorer', command: 'explorer_locate_file' },
  explorer_search: { domain: 'explorer', command: 'explorer_search' },
  set_conversation_explorer_root: {
    domain: 'explorer',
    command: 'set_conversation_explorer_root',
  },
  explorer_index_open: { domain: 'explorer', command: 'explorer_index_open' },
  explorer_index_close: { domain: 'explorer', command: 'explorer_index_close' },
  explorer_index_status: {
    domain: 'explorer',
    command: 'explorer_index_status',
  },
  explorer_index_rebuild: {
    domain: 'explorer',
    command: 'explorer_index_rebuild',
  },
  explorer_folders_list: {
    domain: 'explorer',
    command: 'explorer_folders_list',
  },
  explorer_folder_rename: {
    domain: 'explorer',
    command: 'explorer_folder_rename',
  },
  explorer_folder_set_pinned: {
    domain: 'explorer',
    command: 'explorer_folder_set_pinned',
  },
  explorer_folder_set_settings: {
    domain: 'explorer',
    command: 'explorer_folder_set_settings',
  },
  explorer_folder_delete_index: {
    domain: 'explorer',
    command: 'explorer_folder_delete_index',
  },
  explorer_folder_remove: {
    domain: 'explorer',
    command: 'explorer_folder_remove',
  },

  // Compare domain
  list_study_decks: { domain: 'study', command: 'list_study_decks' },
  get_study_deck: { domain: 'study', command: 'get_study_deck' },
  generate_study_deck: { domain: 'study', command: 'generate_study_deck' },
  generate_conversation_study_deck: {
    domain: 'study',
    command: 'generate_conversation_study_deck',
  },
  review_study_card: { domain: 'study', command: 'review_study_card' },
  update_study_card: { domain: 'study', command: 'update_study_card' },
  delete_study_deck: { domain: 'study', command: 'delete_study_deck' },
  list_learning_programs: {
    domain: 'learning',
    command: 'list_learning_programs',
  },
  get_learning_plan: { domain: 'learning', command: 'get_learning_plan' },
  preview_learning_curriculum_revision: {
    domain: 'learning',
    command: 'preview_learning_curriculum_revision',
  },
  accept_learning_curriculum_revision: {
    domain: 'learning',
    command: 'accept_learning_curriculum_revision',
  },
  discard_learning_curriculum_revision: {
    domain: 'learning',
    command: 'discard_learning_curriculum_revision',
  },
  start_learning_diagnostic: {
    domain: 'learning',
    command: 'start_learning_diagnostic',
  },
  submit_learning_diagnostic: {
    domain: 'learning',
    command: 'submit_learning_diagnostic',
  },
  skip_learning_diagnostic: {
    domain: 'learning',
    command: 'skip_learning_diagnostic',
  },
  start_learning_generation_job: {
    domain: 'learning',
    command: 'start_learning_generation_job',
  },
  cancel_learning_generation_job: {
    domain: 'learning',
    command: 'cancel_learning_generation_job',
  },
  retry_learning_generation_job: {
    domain: 'learning',
    command: 'retry_learning_generation_job',
  },
  get_learning_generation_job: {
    domain: 'learning',
    command: 'get_learning_generation_job',
  },
  get_learning_program: { domain: 'learning', command: 'get_learning_program' },
  get_learning_lesson_evidence: {
    domain: 'learning',
    command: 'get_learning_lesson_evidence',
  },
  generate_learning_program: {
    domain: 'learning',
    command: 'generate_learning_program',
  },
  cancel_learning_outline: {
    domain: 'learning',
    command: 'cancel_learning_outline',
  },
  repair_learning_outline: {
    domain: 'learning',
    command: 'repair_learning_outline',
  },
  accept_learning_program: {
    domain: 'learning',
    command: 'accept_learning_program',
  },
  prepare_learning_lesson: {
    domain: 'learning',
    command: 'prepare_learning_lesson',
  },
  complete_learning_lesson: {
    domain: 'learning',
    command: 'complete_learning_lesson',
  },
  submit_learning_attempt: {
    domain: 'learning',
    command: 'submit_learning_attempt',
  },
  delete_learning_program: {
    domain: 'learning',
    command: 'delete_learning_program',
  },
  get_learning_memory: { domain: 'learning', command: 'get_learning_memory' },
  ensure_learning_lesson_note: {
    domain: 'learning',
    command: 'ensure_learning_lesson_note',
  },
  generate_learning_card_drafts: {
    domain: 'learning',
    command: 'generate_learning_card_drafts',
  },
  save_learning_card_draft: {
    domain: 'learning',
    command: 'save_learning_card_draft',
  },
  accept_learning_card_draft: {
    domain: 'learning',
    command: 'accept_learning_card_draft',
  },
  discard_learning_card_draft: {
    domain: 'learning',
    command: 'discard_learning_card_draft',
  },
  get_learning_canvas_workspace: {
    domain: 'learning',
    command: 'get_learning_canvas_workspace',
  },
  create_learning_canvas: {
    domain: 'learning',
    command: 'create_learning_canvas',
  },
  save_learning_canvas: { domain: 'learning', command: 'save_learning_canvas' },
  create_learning_canvas_snapshot: {
    domain: 'learning',
    command: 'create_learning_canvas_snapshot',
  },
  restore_learning_canvas_snapshot: {
    domain: 'learning',
    command: 'restore_learning_canvas_snapshot',
  },
  get_learning_source_workspace: {
    domain: 'learning',
    command: 'get_learning_source_workspace',
  },
  get_learning_source_version: {
    domain: 'learning',
    command: 'get_learning_source_version',
  },
  search_learning_sources: {
    domain: 'learning',
    command: 'search_learning_sources',
  },
  add_learning_web_source: {
    domain: 'learning',
    command: 'add_learning_web_source',
  },
  add_learning_document_source: {
    domain: 'learning',
    command: 'add_learning_document_source',
  },
  add_learning_text_source: {
    domain: 'learning',
    command: 'add_learning_text_source',
  },
  refresh_learning_source: {
    domain: 'learning',
    command: 'refresh_learning_source',
  },
  adopt_learning_source_version: {
    domain: 'learning',
    command: 'adopt_learning_source_version',
  },
  update_learning_source_policy: {
    domain: 'learning',
    command: 'update_learning_source_policy',
  },
  get_learning_practice_workspace: {
    domain: 'learning',
    command: 'get_learning_practice_workspace',
  },
  get_learning_practice_session: {
    domain: 'learning',
    command: 'get_learning_practice_session',
  },
  start_learning_practice_session: {
    domain: 'learning',
    command: 'start_learning_practice_session',
  },
  save_learning_practice_artifact: {
    domain: 'learning',
    command: 'save_learning_practice_artifact',
  },
  change_learning_practice_mode: {
    domain: 'learning',
    command: 'change_learning_practice_mode',
  },
  open_learning_practice_source: {
    domain: 'learning',
    command: 'open_learning_practice_source',
  },
  request_learning_tutor_response: {
    domain: 'learning',
    command: 'request_learning_tutor_response',
  },
  reveal_learning_practice_solution: {
    domain: 'learning',
    command: 'reveal_learning_practice_solution',
  },
  submit_learning_practice_attempt: {
    domain: 'learning',
    command: 'submit_learning_practice_attempt',
  },
  accept_learning_practice_proposal: {
    domain: 'learning',
    command: 'accept_learning_practice_proposal',
  },
  reject_learning_practice_proposal: {
    domain: 'learning',
    command: 'reject_learning_practice_proposal',
  },
  get_learning_assessment_workspace: {
    domain: 'learning',
    command: 'get_learning_assessment_workspace',
  },
  create_learning_assessment_blueprint: {
    domain: 'learning',
    command: 'create_learning_assessment_blueprint',
  },
  start_learning_assessment_form: {
    domain: 'learning',
    command: 'start_learning_assessment_form',
  },
  get_learning_assessment_form: {
    domain: 'learning',
    command: 'get_learning_assessment_form',
  },
  save_learning_assessment_response: {
    domain: 'learning',
    command: 'save_learning_assessment_response',
  },
  interrupt_learning_assessment_form: {
    domain: 'learning',
    command: 'interrupt_learning_assessment_form',
  },
  submit_learning_assessment_form: {
    domain: 'learning',
    command: 'submit_learning_assessment_form',
  },
  accept_learning_follow_up: {
    domain: 'learning',
    command: 'accept_learning_follow_up',
  },
  dismiss_learning_follow_up: {
    domain: 'learning',
    command: 'dismiss_learning_follow_up',
  },
  get_learning_practical_workspace: {
    domain: 'learning',
    command: 'get_learning_practical_workspace',
  },
  get_learning_practical_draft: {
    domain: 'learning',
    command: 'get_learning_practical_draft',
  },
  save_learning_practical_draft: {
    domain: 'learning',
    command: 'save_learning_practical_draft',
  },
  get_learning_runtime_catalog: {
    domain: 'learning',
    command: 'get_learning_runtime_catalog',
  },
  prepare_learning_runtime_preset: {
    domain: 'learning',
    command: 'prepare_learning_runtime_preset',
  },
  save_learning_runtime_profile: {
    domain: 'learning',
    command: 'save_learning_runtime_profile',
  },
  generate_learning_practical_activity: {
    domain: 'learning',
    command: 'generate_learning_practical_activity',
  },
  start_learning_practical_run: {
    domain: 'learning',
    command: 'start_learning_practical_run',
  },
  cancel_learning_practical_run: {
    domain: 'learning',
    command: 'cancel_learning_practical_run',
  },
  start_learning_simulation: {
    domain: 'learning',
    command: 'start_learning_simulation',
  },
  send_learning_simulation_turn: {
    domain: 'learning',
    command: 'send_learning_simulation_turn',
  },
  finish_learning_simulation: {
    domain: 'learning',
    command: 'finish_learning_simulation',
  },
  get_learning_portability_workspace: {
    domain: 'learning',
    command: 'get_learning_portability_workspace',
  },
  export_learning_pack: { domain: 'learning', command: 'export_learning_pack' },
  preview_learning_pack_import: {
    domain: 'learning',
    command: 'preview_learning_pack_import',
  },
  apply_learning_pack_import: {
    domain: 'learning',
    command: 'apply_learning_pack_import',
  },
  cancel_learning_pack_import_preview: {
    domain: 'learning',
    command: 'cancel_learning_pack_import_preview',
  },
  delete_learning_source: {
    domain: 'learning',
    command: 'delete_learning_source',
  },
  reimport_learning_source: {
    domain: 'learning',
    command: 'reimport_learning_source',
  },
  create_learning_source_selector: {
    domain: 'learning',
    command: 'create_learning_source_selector',
  },
  get_learning_source_selector: {
    domain: 'learning',
    command: 'get_learning_source_selector',
  },
  match_learning_source_selector: {
    domain: 'learning',
    command: 'match_learning_source_selector',
  },
  search_learning_sources_semantically: {
    domain: 'learning',
    command: 'search_learning_sources_semantically',
  },
  get_learning_recall_workspace: {
    domain: 'learning',
    command: 'get_learning_recall_workspace',
  },
  save_learning_recall_card: {
    domain: 'learning',
    command: 'save_learning_recall_card',
  },
  decide_learning_recall_duplicate: {
    domain: 'learning',
    command: 'decide_learning_recall_duplicate',
  },
  change_learning_recall_scheduler: {
    domain: 'learning',
    command: 'change_learning_recall_scheduler',
  },
  review_learning_recall_card: {
    domain: 'learning',
    command: 'review_learning_recall_card',
  },
  compare_documents: { domain: 'compare', command: 'compare_documents' },

  // Credentials domain
  set_huggingface_token: {
    domain: 'huggingface',
    command: 'set_huggingface_token',
  },
  get_huggingface_token_status: {
    domain: 'huggingface',
    command: 'get_huggingface_token_status',
  },
  get_huggingface_token: {
    domain: 'huggingface',
    command: 'get_huggingface_token',
  },
  delete_huggingface_token: {
    domain: 'huggingface',
    command: 'delete_huggingface_token',
  },

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
  clear_completed_downloads: {
    domain: 'download',
    command: 'clear_completed_downloads',
  },
  list_downloads: { domain: 'download', command: 'list_downloads' },
  get_download_status: { domain: 'download', command: 'download_get_status' },

  // Additional model management
  download_model: { domain: 'model', command: 'download_model' },
  get_compatible_models: { domain: 'model', command: 'get_compatible_models' },
  search_model_catalog: { domain: 'model', command: 'search_model_catalog' },
  refresh_model_catalog: { domain: 'model', command: 'refresh_model_catalog' },
  clear_model_catalog_cache: {
    domain: 'model',
    command: 'clear_model_catalog_cache',
  },
  clear_active_chat_model: {
    domain: 'model',
    command: 'clear_active_chat_model',
  },
  clear_active_embedding_model: {
    domain: 'model',
    command: 'clear_active_embedding_model',
  },
  set_active_utility_model: {
    domain: 'model',
    command: 'set_active_utility_model',
  },
  clear_active_utility_model: {
    domain: 'model',
    command: 'clear_active_utility_model',
  },
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
  get_mentions_for_document: {
    domain: 'mention',
    command: 'get_mentions_for_document',
  },
  get_backlinks_for_mention: {
    domain: 'mention',
    command: 'get_backlinks_for_mention',
  },
  get_mentions_by_type: { domain: 'mention', command: 'get_mentions_by_type' },
  create_mention: { domain: 'mention', command: 'create_mention' },
  delete_mention: { domain: 'mention', command: 'delete_mention' },
  generate_embedding: { domain: 'embeddings', command: 'generate_embedding' },
  generate_embeddings_batch: {
    domain: 'embeddings',
    command: 'generate_embeddings_batch',
  },
  get_embedding_model_info: {
    domain: 'embeddings',
    command: 'get_embedding_model_info',
  },
  parse_wikilinks: { domain: 'extraction', command: 'parse_wikilinks' },
  extract_document_title: {
    domain: 'extraction',
    command: 'extract_document_title',
  },
  resolve_wikilink: { domain: 'extraction', command: 'resolve_wikilink' },
  extract_and_resolve_links: {
    domain: 'extraction',
    command: 'extract_and_resolve_links',
  },
  get_settings: { domain: 'settings', command: 'get_settings' },
  get_settings_category: {
    domain: 'settings',
    command: 'get_settings_category',
  },
  update_settings: { domain: 'settings', command: 'update_settings' },
  reset_settings: { domain: 'settings', command: 'reset_settings' },
  export_settings: { domain: 'settings', command: 'export_settings' },
  import_settings: { domain: 'settings', command: 'import_settings' },
  get_system_theme: { domain: 'settings', command: 'get_system_theme' },
  validate_folder_path: { domain: 'settings', command: 'validate_folder_path' },
  test_ollama_connection: {
    domain: 'settings',
    command: 'test_ollama_connection',
  },
  test_llama_cpp_connection: {
    domain: 'settings',
    command: 'test_llama_cpp_connection',
  },
  test_custom_tool: { domain: 'settings', command: 'test_custom_tool' },
  get_today_note: { domain: 'dailynotes', command: 'get_today_note' },
  quick_capture: { domain: 'dailynotes', command: 'quick_capture' },
  get_daily_notes_range: {
    domain: 'dailynotes',
    command: 'get_daily_notes_range',
  },
  get_previous_daily_note: {
    domain: 'dailynotes',
    command: 'get_previous_daily_note',
  },
  get_next_daily_note: { domain: 'dailynotes', command: 'get_next_daily_note' },
  update_daily_note_content: {
    domain: 'dailynotes',
    command: 'update_daily_note_content',
  },
  list_workspace_notes: {
    domain: 'dailynotes',
    command: 'list_workspace_notes',
  },
  create_workspace_note: {
    domain: 'dailynotes',
    command: 'create_workspace_note',
  },
  update_workspace_note: {
    domain: 'dailynotes',
    command: 'update_workspace_note',
  },
  capture_reference: { domain: 'dailynotes', command: 'capture_reference' },
  delete_workspace_note: {
    domain: 'dailynotes',
    command: 'delete_workspace_note',
  },
  add_watch_folder: { domain: 'settings', command: 'add_watch_folder' },
  remove_watch_folder: { domain: 'settings', command: 'remove_watch_folder' },
  ingest_web_url: { domain: 'web', command: 'ingest_web_url' },
  fetch_url_preview: { domain: 'web', command: 'fetch_url_preview' },
  extract_article: { domain: 'web', command: 'extract_article' },
  read_web_page: { domain: 'web', command: 'read_web_page' },
  check_for_updates: { domain: 'updates', command: 'check_for_updates' },
  execute_function: { domain: 'functions', command: 'execute_function' },
  list_available_functions: {
    domain: 'functions',
    command: 'list_available_functions',
  },
  get_function_stats: { domain: 'functions', command: 'get_function_stats' },
  get_version_info: { domain: 'updates', command: 'get_version_info' },
  rescan_vault: { domain: 'vault', command: 'rescan_vault' },

  // Transcription
  transcribe_file: { domain: 'transcription', command: 'transcribe_file' },
  get_transcription_status: {
    domain: 'transcription',
    command: 'get_transcription_status',
  },

  // Corpus shape
  get_corpus_shape: { domain: 'file', command: 'get_corpus_shape' },
  list_conversations_citing_document: {
    domain: 'file',
    command: 'list_conversations_citing_document',
  },
  find_similar_documents: {
    domain: 'search',
    command: 'find_similar_documents',
  },
  cluster_vault_run: { domain: 'corpus-shape', command: 'cluster_vault_run' },
  cluster_vault_debug: {
    domain: 'corpus-shape',
    command: 'cluster_vault_debug',
  },
  list_clusters: { domain: 'corpus-shape', command: 'list_clusters' },

  // Backup and export domain
  plugin_create_backup: { domain: 'backup', command: 'plugin_create_backup' },
  plugin_restore_backup: { domain: 'backup', command: 'plugin_restore_backup' },
  plugin_list_backups: { domain: 'backup', command: 'plugin_list_backups' },
  plugin_export_markdown: {
    domain: 'backup',
    command: 'plugin_export_markdown',
  },
  plugin_export_json: { domain: 'backup', command: 'plugin_export_json' },
  plugin_export_csv: { domain: 'backup', command: 'plugin_export_csv' },
  plugin_export_html: { domain: 'backup', command: 'plugin_export_html' },

  // Off-device backup (encrypted archive) domain
  plugin_get_archive_status: {
    domain: 'backup',
    command: 'plugin_get_archive_status',
  },
  plugin_begin_archive_setup: {
    domain: 'backup',
    command: 'plugin_begin_archive_setup',
  },
  plugin_confirm_archive_setup: {
    domain: 'backup',
    command: 'plugin_confirm_archive_setup',
  },
  plugin_choose_archive_destination: {
    domain: 'backup',
    command: 'plugin_choose_archive_destination',
  },
  plugin_set_archive_keep_count: {
    domain: 'backup',
    command: 'plugin_set_archive_keep_count',
  },
  plugin_set_archive_passphrase: {
    domain: 'backup',
    command: 'plugin_set_archive_passphrase',
  },
  plugin_rotate_recovery_code: {
    domain: 'backup',
    command: 'plugin_rotate_recovery_code',
  },
  plugin_disable_archive: {
    domain: 'backup',
    command: 'plugin_disable_archive',
  },
  plugin_create_archive_now: {
    domain: 'backup',
    command: 'plugin_create_archive_now',
  },
  plugin_restore_archive: {
    domain: 'backup',
    command: 'plugin_restore_archive',
  },
};
