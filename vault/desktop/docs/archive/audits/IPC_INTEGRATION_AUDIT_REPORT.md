# IPC INTEGRATION AUDIT REPORT
**Generated:** 2026-01-23
**Scope:** Frontend-Backend IPC Integration Analysis
**Gateway Pattern Implementation Status**

---

## Executive Summary

This audit analyzed the integration between frontend IPC calls (via `COMMAND_DOMAIN_MAP` in `api.ts`) and backend gateway handlers (in `src/crates/recall/ipc/domains/*.rs`). The gateway pattern is partially implemented, with significant gaps in the FILE domain and complete unavailability of CONFIG domain commands.

**Critical Findings:**
- FILE domain: 6/18 commands implemented (33%)
- CONFIG domain: 0/2 commands implemented (0%) - requires architectural change
- MODEL domain: 11/11 commands implemented (100%)
- SEARCH domain: 6/6 commands implemented (100%)
- HEALTH domain: 3/3 commands implemented (100%)

**Overall Gateway Coverage:** 26/40 commands (65%)

---

## Domain: FILE

**Frontend Mapping Location:** `api.ts` lines 127-161
**Backend Handler:** `src/crates/recall/ipc/domains/file.rs`
**Coverage:** 6/18 commands (33%)

### ✅ Fully Connected (6)

| Command | Status | Notes |
|---------|--------|-------|
| `get_indexing_stats` | ✅ WORKING | Direct SQL query, returns `IndexingStatsDto` |
| `list_all_documents` | ✅ WORKING | Direct SQL query with 10k limit, pagination ready |
| `get_indexing_activities` | ✅ WORKING | Direct SQL query, returns recent indexing events |
| `get_indexed_folders` | ✅ WORKING | Direct SQL query, returns folder watch list |
| `get_recent_documents` | ✅ WORKING | Direct SQL query, ordered by `modified_at DESC` |
| `import_batch` | ✅ WORKING | Batch file import with rate limiting & audit logging |

### ❌ Missing Backend Implementation (12)

| Command | Frontend Maps To | Error Returned |
|---------|------------------|----------------|
| `open_file` | `file/open_file` | "File command 'open_file' not yet available via gateway (use direct Tauri commands)" |
| `open_file_by_id` | `file/open_file_by_id` | "File command 'open_file_by_id' not yet available via gateway (use direct Tauri commands)" |
| `get_file_path_by_id` | `file/get_file_path_by_id` | "File command 'get_file_path_by_id' not yet available via gateway (use direct Tauri commands)" |
| `get_file_metadata` | `file/get_file_metadata` | "File command 'get_file_metadata' not yet available via gateway (use direct Tauri commands)" |
| `read_file_content` | `file/read_file_content` | "File command 'read_file_content' not yet available via gateway (use direct Tauri commands)" |
| `show_in_folder` | `file/show_in_folder` | "File command 'show_in_folder' not yet available via gateway (use direct Tauri commands)" |
| `remove_indexed_folder` | `file/remove_indexed_folder` | "File command 'remove_indexed_folder' not yet available via gateway (use direct Tauri commands)" |
| `index_directory` | `file/index_directory` | "File command 'index_directory' not yet available via gateway (use direct Tauri commands)" |
| `index_file` | `file/index_file` | "File command 'index_file' not yet available via gateway (use direct Tauri commands)" |
| `reindex_file` | `file/reindex_file` | "File command 'reindex_file' not yet available via gateway (use direct Tauri commands)" |
| `remove_indexed_file` | `file/remove_indexed_file` | "File command 'remove_indexed_file' not yet available via gateway (use direct Tauri commands)" |
| `delete_document` | `file/delete_document` | "File command 'delete_document' not yet available via gateway (use direct Tauri commands)" |

**Additional Missing Commands:**
- `get_index_progress` - frontend maps to `file/get_index_progress` (line 158)
- `cancel_indexing` - frontend maps to `file/cancel_indexing` (line 159)
- `get_document` - frontend maps to `file/get_document` (line 161)

### ⚠️  Unmapped Commands (0)
*No backend-only commands found*

### 🔧 Integration Issues (2)

1. **`import_batch` argument mismatch**
   - Frontend expects: `{ filePaths: string[] }` (camelCase)
   - Backend extracts: `extract_string_array(&args, "filePaths")` (camelCase)
   - **Status:** ✅ ALIGNED

2. **Missing `get_document` implementation**
   - Frontend: Line 638-639 calls `apiCall<DocumentMetadata>('get_document', { documentId })`
   - Backend: Returns error "not yet available via gateway"
   - **Impact:** DocumentViewer cannot fetch document metadata by ID
   - **Workaround:** Use direct Tauri command fallback

---

## Domain: SEARCH

**Frontend Mapping Location:** `api.ts` lines 119-125
**Backend Handler:** `src/crates/recall/ipc/domains/search.rs`
**Coverage:** 6/6 commands (100%)

### ✅ Fully Connected (6)

| Command | Status | Args Format | Returns |
|---------|--------|-------------|---------|
| `search_documents` | ✅ WORKING | `{ options: SearchOptions }` | `SearchResult[]` |
| `search_fast` | ✅ WORKING | `{ query: string, limit: number }` | `SearchResult[]` |
| `semantic_search` | ✅ WORKING | `{ request: SearchRequestDto }` | `SearchResult[]` |
| `hybrid_search` | ✅ WORKING | `{ query: string, limit: number, searchMode: string }` | `SearchResult[]` |
| `find_similar` | ✅ WORKING | `{ chunkId: string, limit?: number }` | `SearchResult[]` |
| `search_with_recency` | ✅ WORKING | `{ options: RecencySearchOptions }` | `SearchResult[]` |

### ❌ Missing Backend Implementation (0)
*All search commands implemented*

### ⚠️  Unmapped Commands (0)
*No backend-only commands found*

### 🔧 Integration Issues (0)
*No issues detected - domain is fully functional*

---

## Domain: MODEL

**Frontend Mapping Location:** `api.ts` lines 105-117
**Backend Handler:** `src/crates/recall/ipc/domains/model.rs`
**Coverage:** 11/11 commands (100%)

### ✅ Fully Connected (11)

| Command | Status | Security Features | Notes |
|---------|--------|-------------------|-------|
| `get_models_with_metadata` | ✅ WORKING | Rate limiting, audit logging | Returns `DownloadedModelResponse[]` |
| `is_model_already_downloaded` | ✅ WORKING | Rate limiting, input validation | Validates model ID format |
| `set_active_chat_model` | ✅ WORKING | Rate limiting, audit logging, cache invalidation | Clears LLM cache on change |
| `get_active_chat_model` | ✅ WORKING | Rate limiting | Returns `DownloadedModel \| null` |
| `delete_downloaded_model_and_file` | ✅ WORKING | Rate limiting, audit logging | Supports `deleteFile` boolean |
| `get_active_embedding_model` | ✅ WORKING | Rate limiting | Returns `DownloadedModel \| null` |
| `set_active_embedding_model` | ✅ WORKING | Rate limiting, audit logging, cache invalidation | Clears embedding cache on change |
| `download_model_command` | ✅ WORKING | Rate limiting, audit logging | Maps to `download` internally |
| `get_download_status` | ✅ WORKING | Rate limiting | Returns `DownloadStatus \| null` |
| `detect_system_capabilities` | ✅ WORKING | None | Detects GPU, RAM, CPU, architecture |
| `get_all_recommended_models` | ✅ WORKING | None | Filters by system compatibility |

### ❌ Missing Backend Implementation (0)
*All model commands implemented*

### ⚠️  Unmapped Commands (2)

| Command | Backend Implementation | Frontend Mapping |
|---------|----------------------|------------------|
| `clear_active_chat_model` | ✅ Implemented | ❌ Not mapped in `COMMAND_DOMAIN_MAP` |
| `clear_active_embedding_model` | ✅ Implemented | ❌ Not mapped in `COMMAND_DOMAIN_MAP` |

**Recommendation:** Add frontend mappings:
```typescript
'clear_active_chat_model': { domain: 'model', command: 'clear_active_chat_model' },
'clear_active_embedding_model': { domain: 'model', command: 'clear_active_embedding_model' },
```

### 🔧 Integration Issues (1)

1. **`get_model_catalog_stats` returns placeholder data**
   - Frontend: Line 2248 expects `ModelCatalogCacheStats`
   - Backend: Returns `{ "total_entries": 0, "valid_entries": 0, "expired_entries": 0 }`
   - **Impact:** Model catalog cache statistics always show zero
   - **Status:** Placeholder implementation acknowledged in code (line 452-459)

---

## Domain: CONFIG

**Frontend Mapping Location:** `api.ts` lines 144-145
**Backend Handler:** `src/crates/recall/ipc/domains/config.rs`
**Coverage:** 0/2 commands (0%)

### ✅ Fully Connected (0)
*No commands implemented via gateway*

### ❌ Missing Backend Implementation (2)

| Command | Frontend Maps To | Error Returned |
|---------|------------------|----------------|
| `get_config` | `config/get_config` | "Config command 'get_config' not yet available via gateway (requires ConfigService state access)" |
| `save_config` | `config/save_config` | "Config command 'save_config' not yet available via gateway (requires ConfigService state access)" |

### ⚠️  Unmapped Commands (0)
*No backend-only commands found*

### 🔧 Integration Issues (1)

1. **ConfigService State Access Problem (ARCHITECTURAL)**
   - **Root Cause:** ConfigService is managed separately in Tauri state, not in DI Container
   - **Impact:** Gateway pattern cannot access ConfigService with current architecture
   - **Code Reference:** `config.rs` lines 18-22 (TODO comment)
   - **Solutions:**
     - **Option A:** Move ConfigService into DI Container (requires refactoring)
     - **Option B:** Update gateway to support multiple state types
     - **Option C:** Keep config commands as direct Tauri commands (status quo)
   - **Current Workaround:** Frontend uses `spectaCommands.getConfig()` directly (see `api.ts` line 682)

---

## Domain: HEALTH

**Frontend Mapping Location:** `api.ts` lines 139-148
**Backend Handler:** `src/crates/recall/ipc/domains/health.rs`
**Coverage:** 3/3 commands (100%)

### ✅ Fully Connected (3)

| Command | Status | Implementation |
|---------|--------|---------------|
| `health_check` | ✅ WORKING | Delegates to `health_commands::health_check_impl()` |
| `get_system_stats` | ✅ WORKING | Delegates to `health_commands::get_system_stats_impl()` |
| `get_version` | ✅ WORKING | Delegates to `health_commands::get_version()` |

**Additional Command:**
- `initialize_database` - Mapped to health domain (line 148), returns success no-op

### ❌ Missing Backend Implementation (0)
*All health commands implemented*

### ⚠️  Unmapped Commands (0)
*No backend-only commands found*

### 🔧 Integration Issues (1)

1. **`initialize_database` routing ambiguity**
   - Frontend: Maps to `health/initialize_database` (line 148)
   - Backend: Returns `"Database initialized"` as no-op (lines 54-58)
   - **Note:** Database is initialized during app setup, not via IPC
   - **Impact:** None - this is intentional behavior

---

## Cross-Domain Analysis

### Security & Performance

| Domain | Rate Limiting | Audit Logging | Input Validation | Cache Management |
|--------|---------------|---------------|------------------|------------------|
| FILE | ✅ Yes (batch import only) | ✅ Yes (batch import) | ✅ Yes (path validation) | N/A |
| SEARCH | ❌ No | ❌ No | ❌ No | N/A |
| MODEL | ✅ Yes (all commands) | ✅ Yes (mutations) | ✅ Yes (model IDs) | ✅ Yes (LLM & embedding) |
| CONFIG | N/A (not implemented) | N/A | N/A | N/A |
| HEALTH | ❌ No | ❌ No | ❌ No | N/A |

**Recommendation:** Add rate limiting to SEARCH domain to prevent abuse.

### Error Handling Patterns

| Domain | Error Format | Consistent | Notes |
|--------|--------------|------------|-------|
| FILE | `String` errors | ✅ Yes | Format: "Failed to {action}: {error}" |
| SEARCH | `String` errors | ✅ Yes | Uses `.to_string()` on errors |
| MODEL | `String` errors | ✅ Yes | Comprehensive error messages |
| CONFIG | `String` errors | ✅ Yes | N/A (not implemented) |
| HEALTH | `String` errors | ✅ Yes | JSON parse errors handled |

### Stack Overflow Mitigation

**Good Practice Observed:** FILE and SEARCH domains use direct SQL queries instead of repository entity mapping to avoid stack overflow issues (see `file.rs` lines 129-150).

**Example:**
```rust
// Direct SQL (avoids stack issues)
let documents = sqlx::query_as::<_, ListDocumentRow>(
    "SELECT id, file_name, file_path, ... FROM documents ORDER BY indexed_at DESC LIMIT ?"
)
.bind(safe_limit as i64)
.fetch_all(container.db_pool())
.await
```

---

## Recommendations

### Priority 1: FILE Domain Completion (HIGH IMPACT)

**Missing Critical Commands:**
1. `open_file` - Essential for document viewer
2. `open_file_by_id` - Used by FileBrowser component
3. `get_document` - Required for document metadata display
4. `index_file` - Core indexing functionality
5. `delete_document` - Essential for document management

**Implementation Path:**
- Create async implementations following `list_all_documents_impl` pattern
- Use direct SQL queries to avoid stack overflow
- Add rate limiting and audit logging
- Implement proper error handling with descriptive messages

### Priority 2: CONFIG Domain Architecture Fix (MEDIUM IMPACT)

**Current Blocker:** ConfigService not accessible from gateway

**Recommended Solution:**
1. Move ConfigService into DI Container
2. Add config_service() method to Container
3. Update config domain adapter to use container.config_service()

**Alternative:** Keep config commands as direct Tauri commands (low priority since `getConfig()` already works via Specta)

### Priority 3: MODEL Domain Polish (LOW IMPACT)

**Missing Mappings:**
- Add `clear_active_chat_model` to `COMMAND_DOMAIN_MAP`
- Add `clear_active_embedding_model` to `COMMAND_DOMAIN_MAP`

**Placeholder Implementation:**
- Implement real `get_model_catalog_stats` logic (currently returns zeros)

### Priority 4: Security Hardening (MEDIUM IMPACT)

**Add Rate Limiting:**
- SEARCH domain: `search_documents`, `semantic_search`, `hybrid_search`
- FILE domain: `open_file`, `read_file_content`

**Add Input Validation:**
- SEARCH domain: Validate query strings (max length, sanitization)
- FILE domain: Path traversal protection for `read_file_content`

---

## Testing Recommendations

### Integration Tests Needed

1. **FILE Domain:**
   ```typescript
   // Test missing commands return helpful errors
   const result = await VaultAPI.openFile('/path/to/file.pdf');
   expect(result.ok).toBe(false);
   expect(result.error).toContain('not yet available via gateway');
   ```

2. **MODEL Domain:**
   ```typescript
   // Test unmapped clear commands exist in backend
   const clearChatResult = await invoke('gateway_command', {
     request: { domain: 'model', command: 'clear_active_chat_model', args: {} }
   });
   expect(clearChatResult).toBeDefined();
   ```

3. **CONFIG Domain:**
   ```typescript
   // Test config commands fail gracefully
   const result = await VaultAPI.getConfig();
   // Should use Specta fallback, not gateway
   expect(result.ok).toBe(true);
   ```

### End-to-End Scenarios

1. **Document Opening Workflow:**
   - Search for document → Get document by ID → Open file
   - **Current Status:** Breaks at "Open file" (not implemented)

2. **Model Management Workflow:**
   - Detect capabilities → Get recommendations → Download model → Set active
   - **Current Status:** ✅ WORKING

3. **Batch Import Workflow:**
   - Select files → Start batch import → Track progress
   - **Current Status:** ✅ WORKING (uses gateway `import_batch`)

---

## Appendix: Command Inventory

### All Frontend-Mapped Commands (40 total)

**FILE Domain (18):**
- get_indexing_stats ✅
- list_all_documents ✅
- get_indexing_activities ✅
- get_indexed_folders ✅
- get_recent_documents ✅
- import_batch ✅
- open_file ❌
- open_file_by_id ❌
- get_file_path_by_id ❌
- get_file_metadata ❌
- read_file_content ❌
- show_in_folder ❌
- remove_indexed_folder ❌
- index_directory ❌
- index_file ❌
- reindex_file ❌
- remove_indexed_file ❌
- delete_document ❌

**SEARCH Domain (6):**
- search_documents ✅
- search_fast ✅
- semantic_search ✅
- hybrid_search ✅
- find_similar ✅
- search_with_recency ✅

**MODEL Domain (11):**
- get_models_with_metadata ✅
- is_model_already_downloaded ✅
- set_active_chat_model ✅
- get_active_chat_model ✅
- delete_downloaded_model_and_file ✅
- get_active_embedding_model ✅
- set_active_embedding_model ✅
- download_model_command ✅
- get_download_status ✅
- detect_system_capabilities ✅
- get_all_recommended_models ✅

**CONFIG Domain (2):**
- get_config ❌
- save_config ❌

**HEALTH Domain (3):**
- health_check ✅
- get_system_stats ✅
- get_version ✅

### Backend-Only Commands (Not Mapped)

**MODEL Domain:**
- clear_active_chat_model ⚠️ (exists but not mapped)
- clear_active_embedding_model ⚠️ (exists but not mapped)
- download (internal alias for download_model_command)

---

## Conclusion

The gateway pattern implementation is 65% complete (26/40 commands). The SEARCH, MODEL, and HEALTH domains are fully functional, while FILE and CONFIG domains require significant work.

**Key Blockers:**
1. FILE domain: 12 critical commands not implemented
2. CONFIG domain: Architectural issue prevents implementation
3. MODEL domain: 2 commands implemented but not exposed to frontend

**Next Steps:**
1. Prioritize FILE domain command implementations (Phase 1)
2. Resolve CONFIG domain architecture issue (Phase 2)
3. Add missing MODEL domain mappings (Phase 3)
4. Implement security hardening (Phase 4)

**Impact Assessment:**
- **High Impact:** FILE domain gaps affect core document management workflows
- **Medium Impact:** CONFIG domain gap requires Specta fallback (already in place)
- **Low Impact:** MODEL domain missing mappings are non-critical features

---

**Report Generated By:** IPC Integration Audit Tool
**Date:** 2026-01-23
**Version:** 1.0.0
