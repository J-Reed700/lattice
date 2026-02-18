# Search Domain Migration Specification

## OPERATION UNIFICATION - Search Domain Phase

**Status**: ⚠️ **BLOCKED - Awaiting Oracle Guidance**

**Blocker**: Search Plugin commands exist in bindings.ts but ALL return `NotImplemented` errors (stubs).

---

## Executive Summary

Per Oracle's Domain-Centric Strangler Pattern, the Search Domain migration was selected as the first domain to migrate from VaultAPI (legacy gateway) to Specta bindings (plugin architecture).

**Critical Discovery**: Backend Search Plugin is NOT implemented - all 6 commands are stubs returning `ErrorCode::NotImplemented`.

---

## Audit Results

### Frontend Legacy Search Invocations: 2 Found

#### 1. MentionAutocomplete.tsx:42
**Location**: `websrc/components/MentionAutocomplete/MentionAutocomplete.tsx`

**Current Implementation**:
```typescript
const result = await invoke<{ mentions: Mention[] }>('search_mentions', {
  query: searchQuery,
  limit: 10,
});
```

**Use Case**: Autocomplete search for mentions (@person, @concept) and wikilinks ([[note]])
- Filters results by type: `'person' | 'concept' | 'wikilink'`
- Returns `{ mentions: Mention[] }` with id, name, type, metadata, createdAt

**Migration Target**: ❓ Unclear - no direct equivalent in Search Plugin
- Option: `semanticSearch()` with post-filter by type?
- Option: New command `searchMentions()` needed?

---

#### 2. useOptimizedSearch.ts:70
**Location**: `websrc/hooks/useOptimizedSearch.ts`

**Current Implementation**:
```typescript
const searchResults = await invoke('search_documents', {
  options: {
    query,
    limit,
    search_mode: mode,  // 'semantic' | 'keyword' | 'hybrid'
  },
});
```

**Use Case**: Optimized document search with debouncing (300ms) and abort controller
- Supports 3 search modes: semantic, keyword, hybrid
- Returns `SearchResult[]`

**Migration Target**: ✅ Clear mapping to Search Plugin commands
- `search_mode: 'semantic'` → `commands.semanticSearch(query, limit)`
- `search_mode: 'keyword'` → `commands.keywordSearch(query, limit)`
- `search_mode: 'hybrid'` → `commands.hybridSearch(query, limit)`

---

## Backend Analysis

### Search Plugin Status

**Location**: `src/crates/recall/plugins/search/`

**Commands Exported** (6 total):
1. `semantic_search(query, limit)` - ❌ STUB (NotImplemented)
2. `hybrid_search(query, limit)` - ❌ STUB (NotImplemented)
3. `keyword_search(query, limit)` - ❌ STUB (NotImplemented)
4. `search_by_tags(tags, limit)` - ❌ STUB (NotImplemented)
5. `get_search_suggestions(prefix)` - ❌ STUB (NotImplemented)
6. `clear_search_cache()` - ❌ STUB (NotImplemented)

**Current Implementation** (all commands):
```rust
#[tauri::command]
#[specta::specta]
pub async fn semantic_search(
    query: String,
    limit: Option<usize>,
    _container: State<'_, Container>,
) -> Result<Vec<SearchResult>, ApiError> {
    Err(ApiError {
        code: ErrorCode::NotImplemented,
        message: format!("semantic_search not yet implemented for query: {}, limit: {:?}", query, limit),
        details: None,
    })
}
```

### Legacy Gateway Implementation

**Location**: `src/crates/recall/ipc/domains/search.rs`

**Status**: ✅ Working implementation (powers current frontend)
- Handles `search_mentions` command
- Handles `search_documents` command
- Delegates to actual search domain logic

---

## Migration Blocker

**Problem**: Cannot migrate frontend to Specta bindings because backend plugin commands are not implemented.

**Questions for Oracle**:
1. Should we implement the Search Plugin commands first, then migrate frontend?
2. Should we skip Search Domain and move to File Domain?
3. Should we do a vertical slice (implement only 2-3 commands needed for migration)?
4. Something else entirely?

---

## Proposed Migration Options

### Option A: IMPLEMENT SEARCH PLUGIN FIRST
**Steps**:
1. Wire up 6 stub commands in `plugins/search/commands.rs` to delegate to existing search domain logic
2. Test backend implementation
3. Migrate frontend (2 files) to use plugin commands
4. Verify migration with TypeScript build
5. Delete `ipc/domains/search.rs`

**Pros**: Complete vertical slice, clean strangler pattern
**Cons**: Backend work required before frontend migration

---

### Option B: SKIP SEARCH DOMAIN
**Steps**:
1. Move to File Domain migration instead
2. Come back to Search Domain after File is complete

**Pros**: Unblocked immediately, maintain momentum
**Cons**: Leaves Search Domain in legacy state longer

---

### Option C: MINIMAL VERTICAL SLICE
**Steps**:
1. Implement ONLY the 2-3 commands needed:
   - `semantic_search` (for useOptimizedSearch.ts semantic mode)
   - `hybrid_search` (for useOptimizedSearch.ts hybrid mode)
   - `keyword_search` (for useOptimizedSearch.ts keyword mode)
2. Leave `search_mentions`, `search_by_tags`, `get_search_suggestions`, `clear_search_cache` as stubs
3. Migrate useOptimizedSearch.ts only
4. Handle MentionAutocomplete.tsx separately (needs `search_mentions` command or workaround)

**Pros**: Fastest path to partial migration
**Cons**: Incomplete domain migration, leaves some legacy code

---

## Oracle's Strategic Decision ✅

**Guidance Received**: 2026-01-26

**Decision**: **STRICT BOUNDED CONTEXT** - Implement 3 commands, defer 3, exclude `search_mentions`

### 1. `search_mentions` Handling: **EXCLUDE** ❌
- **Ruling**: `search_mentions` belongs to **Mentions Domain**, NOT Search Domain
- **Reasoning**: In DDD, the *entity* being searched defines the domain, not the verb "search"
- **Action**: Leave `search_mentions` in legacy gateway. It will be migrated in future "Operation Unification: Mentions Domain" phase
- **Frontend Impact**: MentionAutocomplete.tsx:42 will continue using legacy `invoke('search_mentions')` - no changes needed

### 2. Missing Commands: **DEFER (STUB)** ⏸️
Commands without existing business logic implementation:
- `search_by_tags` → Return NotImplemented
- `get_search_suggestions` → Return NotImplemented
- `clear_search_cache` → Return NotImplemented

**Reasoning**: Do NOT implement new business logic (features) during a migration (refactor). Expanding scope risks the migration.

### 3. Validated Commands to Implement: **DIRECT MAPPING** ✅
**These 3 commands have existing business logic and will be wired up:**

1. **`semantic_search`** → `semantic_search_impl()`
   - Plugin: `semantic_search(query: String, limit: Option<usize>)` → `Result<Vec<SearchResult>, ApiError>`
   - Business Logic: `semantic_search_impl(&Container, SearchRequestDto)` → `Result<SearchResponseDto, AppError>`
   - Conversion needed: `SearchResponseDto` → `Vec<SearchResult>`

2. **`hybrid_search`** → `hybrid_search_impl()`
   - Plugin: `hybrid_search(query: String, limit: Option<usize>)` → `Result<Vec<SearchResult>, ApiError>`
   - Business Logic: `hybrid_search_impl(&Container, String, usize, String)` → `Result<Vec<SearchResultDto>, AppError>`
   - Conversion needed: `Vec<SearchResultDto>` → `Vec<SearchResult>`

3. **`keyword_search`** → `hybrid_search_impl()` with `SearchMode::Keyword`
   - Plugin: `keyword_search(query: String, limit: Option<usize>)` → `Result<Vec<SearchResult>, ApiError>`
   - Business Logic: `hybrid_search_impl(&Container, String, usize, "keyword")` → `Result<Vec<SearchResultDto>, AppError>`
   - Conversion needed: `Vec<SearchResultDto>` → `Vec<SearchResult>`

---

## Files Identified for Migration

### Backend (Plugin Implementation)
**File**: `src/src/crates/recall/plugins/search/commands.rs`

**Actions**:
1. ✅ Implement `semantic_search()` - Wire to `semantic_search_impl()`
2. ✅ Implement `hybrid_search()` - Wire to `hybrid_search_impl()`
3. ✅ Implement `keyword_search()` - Wire to `hybrid_search_impl()` with `SearchMode::Keyword`
4. ⏸️ Keep `search_by_tags()` as NotImplemented stub
5. ⏸️ Keep `get_search_suggestions()` as NotImplemented stub
6. ⏸️ Keep `clear_search_cache()` as NotImplemented stub

### Frontend (Migration Target)
**File**: `websrc/hooks/useOptimizedSearch.ts:70`

**Current Code**:
```typescript
const searchResults = await invoke('search_documents', {
  options: { query, limit, search_mode }  // 'semantic' | 'keyword' | 'hybrid'
});
```

**Migration Strategy**:
```typescript
// Map legacy search_mode to plugin commands
const result = mode === 'semantic'
  ? await commands.semanticSearch(query, limit)
  : mode === 'keyword'
  ? await commands.keywordSearch(query, limit)
  : await commands.hybridSearch(query, limit);

if (result.status === 'ok') {
  return result.data;  // Vec<SearchResult>
} else {
  throw new Error(getAppErrorMessage(result.error));
}
```

**Out of Scope**:
- ❌ `websrc/components/MentionAutocomplete/MentionAutocomplete.tsx` - `search_mentions` is Mentions Domain, NOT Search Domain

---

## Type Definitions

### SearchResult (Backend)
```rust
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
}
```

### SearchResult (Frontend - from types/index.ts)
```typescript
export interface SearchResult {
  id: string;
  score: f32;
  // Additional fields may exist in frontend type
}
```

⚠️ **Note**: Need to verify frontend SearchResult type matches backend type for successful migration.

---

**Document Version**: 1.0
**Last Updated**: 2026-01-26
**Status**: BLOCKED - Awaiting Oracle guidance on implementation strategy
