# Conversation Spaces Architecture

## Date
- 2026-02-09

## Status
- Draft architecture for implementation kickoff

## Summary
This design introduces a conversation operating model built around:
- `Spaces`: collection-like environments with custom prompt + model defaults
- `Shelf`: saved/bookmarked conversation management
- `Spotlight`: search across conversation history, with saved/bookmarked filters
- `Feed`: resurfacing high-value threads (next phase)

It is designed to fit the current Recall desktop stack (`Rust + Tauri + SQLite + React`) with minimal disruption to existing conversation/chat flows.

## Product Goals
- Organize conversations into durable environments (like mini workspaces).
- Support per-space custom instructions and defaults ("its own little environment").
- Make conversation retrieval first-class: save, bookmark, pin, archive.
- Enable fast search through all chats, especially saved/bookmarked chats.
- Keep interaction sexy: quick, visual, and keyboard-first.

## Competitive Patterns To Borrow
- Perplexity-style spaces: scoped context + instructions + thread grouping.
- ChatGPT/Claude-style project memory boundaries and chat organization.
- Notion/NotebookLM-style retrieval and saved artifacts.

## Current State (Recall)
- `conversations` has core metadata (`title`, `model_name`, `system_prompt`, counters).
- `conversation_messages` stores full message history + metadata.
- UI sidebar is chronological only; no conversation bookmark/save/search primitives.
- No FTS index for conversation text; search is document-centric.

## Proposed Experience

### 1) Spaces (Environment Layer)
- Spaces are top-level containers for conversation organization.
- Every conversation belongs to one space.
- Space defines defaults:
  - `space_prompt`
  - `default_model_name`
  - `tool_preferences_json` (e.g., web-search on/off, KB on/off)
  - optional retrieval scope for future smart collections

### 2) Shelf (Curation Layer)
- Conversation-level states:
  - `saved` (curated long-term)
  - `bookmarked` (quick star)
  - `pinned` (surface to top in current space)
  - `archived` (hidden from active list, still searchable)
- Message-level bookmarks:
  - Save specific assistant/user messages as reusable snippets.

### 3) Spotlight (Retrieval Layer)
- Dedicated conversation search that can filter by:
  - space
  - saved only
  - bookmarked only
  - archived include/exclude
  - date range
  - has message bookmarks
- Returns ranked conversation hits + message snippets.

### 4) Feed (Phase 2)
- Daily resurfacing cards:
  - stale saved threads
  - recently active spaces
  - bookmarked-but-unfinished threads
  - "continue this thread" suggestions

## Domain Model

### New Entity: ConversationSpace
- `id: TEXT (UUID)`
- `name: TEXT`
- `description: TEXT NULL`
- `icon: TEXT NULL` (emoji or icon key)
- `accent_color: TEXT NULL`
- `space_prompt: TEXT NULL`
- `default_model_name: TEXT NULL`
- `tool_preferences_json: TEXT NULL`
- `is_archived: INTEGER DEFAULT 0`
- `sort_order: INTEGER DEFAULT 0`
- `created_at: TEXT`
- `updated_at: TEXT`

### Conversation Enhancements
Add to `conversations`:
- `space_id: TEXT NULL` (required after backfill)
- `is_saved: INTEGER DEFAULT 0`
- `is_bookmarked: INTEGER DEFAULT 0`
- `is_pinned: INTEGER DEFAULT 0`
- `is_archived: INTEGER DEFAULT 0`
- `saved_at: TEXT NULL`
- `bookmarked_at: TEXT NULL`
- `pinned_at: TEXT NULL`
- `archived_at: TEXT NULL`

### New Entity: ConversationMessageBookmark
- `id: TEXT (UUID)`
- `conversation_id: TEXT`
- `message_id: TEXT`
- `title: TEXT NULL`
- `note: TEXT NULL`
- `created_at: TEXT`
- unique constraint on `(conversation_id, message_id)`

## SQLite Schema Plan

### Migration 1: Spaces + Conversation Status
- Create `conversation_spaces`.
- Insert default space row (`General`).
- Add status columns to `conversations`.
- Backfill all existing conversations into `General`.
- Add indexes:
  - `idx_conversations_space_updated(space_id, updated_at DESC)`
  - `idx_conversations_saved(space_id, is_saved, updated_at DESC)`
  - `idx_conversations_bookmarked(space_id, is_bookmarked, updated_at DESC)`
  - `idx_conversations_pinned(space_id, is_pinned, pinned_at DESC)`

### Migration 2: Message Bookmarks
- Create `conversation_message_bookmarks`.
- Add indexes:
  - `idx_message_bookmarks_conversation(conversation_id, created_at DESC)`
  - `idx_message_bookmarks_message(message_id)`

### Migration 3: Conversation FTS
- Create FTS table:
  - `conversation_messages_fts(message_id UNINDEXED, conversation_id UNINDEXED, content)`
- Add insert/update/delete triggers on `conversation_messages`.
- Optional: auxiliary `conversations_fts(conversation_id UNINDEXED, title)` for title-weighting.

## Prompt Resolution Rules (Space Environment)
Effective system prompt for each request:
1. Global settings prompt (`settings.llm.system_prompt`) if present.
2. Space prompt (`conversation_spaces.space_prompt`) if present.
3. Conversation prompt override (`conversations.system_prompt`) if present.

Composition approach:
- Concatenate in order with explicit section delimiters.
- Hard max prompt length guardrail.
- Persist the effective prompt hash in message metadata for debugging/audit.

## Command/API Design (Tauri Plugin Surface)

### Space Commands
- `create_space(request)`
- `list_spaces()`
- `update_space(request)`
- `delete_space(request)` (soft delete/archive recommended)
- `set_active_space(request)` (UI state convenience)

### Conversation Management Extensions
- `move_conversation_to_space(request)`
- `set_conversation_saved(request)`
- `set_conversation_bookmarked(request)`
- `set_conversation_pinned(request)`
- `set_conversation_archived(request)`

### Message Bookmark Commands
- `bookmark_conversation_message(request)`
- `unbookmark_conversation_message(request)`
- `list_message_bookmarks(query)`

### Spotlight Search
- `search_conversations(query)`
  - query includes text + filters (`spaceId`, `savedOnly`, `bookmarkedOnly`, `includeArchived`, pagination)
  - response includes:
    - `conversation` metadata
    - top hit snippets
    - score

## Frontend Architecture

### IA / Layout
Convert chat area to 3-pane:
1. Spaces rail (icons + counts)
2. Conversation explorer (tabs: `All`, `Saved`, `Bookmarked`, `Archived`, search input)
3. Active chat panel

### Key UX Behaviors
- One-click save/bookmark/pin per conversation row.
- Quick search in explorer (debounced).
- Keyboard:
  - `Cmd/Ctrl+K`: Spotlight conversation search
  - `S`: save toggle
  - `B`: bookmark toggle
  - `P`: pin toggle
- Message action menu: `Bookmark message`.

### Frontend Files To Extend
- `websrc/components/Chat/ConversationSidebar.tsx`
- `websrc/stores/conversationsStore.ts`
- `websrc/types/conversation.ts`
- `websrc/lib/api.ts`
- New:
  - `websrc/stores/spacesStore.ts`
  - `websrc/components/Chat/SpacesRail.tsx`
  - `websrc/components/Chat/ConversationSearchInput.tsx`
  - `websrc/components/Chat/ConversationFilters.tsx`

## Backend / Rust Files To Extend
- `src/src/crates/recall/application/dtos/conversation_dto.rs`
- `src/src/crates/recall/application/ports/conversation_repository_port.rs`
- `src/src/crates/recall/infrastructure/persistence/repositories/conversation_repository.rs`
- `src/src/crates/recall/infrastructure/services/traits/conversation.rs`
- `src/src/crates/recall/infrastructure/services/conversation_service.rs`
- `src/src/crates/recall/interfaces/commands/conversation.rs`
- `src/src/crates/recall/plugins/conversation_plugin.rs`
- New:
  - `src/src/crates/recall/domain/conversation_space.rs`
  - `src/src/crates/recall/application/dtos/space_dto.rs`
  - `src/src/crates/recall/interfaces/commands/conversation_spaces.rs` (or extend existing module)

## Rollout Plan

### Phase 1 (Foundation)
- Spaces schema + basic CRUD.
- Conversation save/bookmark/pin/archive flags.
- Sidebar filters (no full-text conversation search yet).
- Prompt stacking from space + conversation.

### Phase 2 (Retrieval Power)
- Message bookmarks.
- Conversation FTS + `search_conversations`.
- Spotlight UI + keyboard flow.

### Phase 3 (Feed + Smart Collections)
- Resurfacing feed.
- Smart collection predicates (rules over conversation metadata and activity).
- Collection-level retrieval scope and automation hooks.

## Test Strategy
- SQL migration tests:
  - backfill correctness
  - trigger sync for FTS
- Repository tests:
  - filter combinations
  - save/bookmark toggles
  - move conversation between spaces
- Command integration tests:
  - end-to-end command payloads
  - permission/rate-limit behavior
- Frontend tests:
  - sidebar filters
  - spotlight query behavior
  - bookmark/save UX state consistency

## Performance and Safety
- FTS queries capped (`limit <= 100`) with pagination.
- All search/filter inputs validated.
- Keep status updates idempotent.
- Add partial indexes for common shelf filters.
- Use soft archive instead of hard delete for safer lifecycle.

## Decisions Made
- One conversation belongs to one primary space.
- Conversation-level `saved` and `bookmarked` are separate states.
- Message bookmarks are first-class entities.
- Conversation retrieval uses FTS5 on message content.

## Open Questions
- Should a conversation be allowed in multiple spaces (many-to-many) later?
- Should saved/bookmarked be unified into one primitive with labels?
- Should space-level retrieval scope be explicit in V1 or V1.5?

## Immediate Next Step
Implement Phase 1 schema + DTO/API surface behind feature flags, then ship the new sidebar explorer before enabling Spotlight FTS.
