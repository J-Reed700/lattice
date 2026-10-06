# Conversation tangents

Chat and Explorer both render `ChatPanel`. Its `ConversationTangents` wrapper owns the tangent drawer. Every completed assistant reply has a visible **Tangent** action alongside Copy and Reference, which starts from that reply. For replies over 8,000 characters, an editable passage picker lets the user narrow the starting point without needing to highlight text.

Highlighting text also reveals **Ask in a tangent** beside the passage, without moving focus or interrupting selection. The same action remains in the right-click menu. A selection must stay within one completed answer; other selections and context menus retain their normal behavior. These entry points all use the same creation and drawer flow.

Tangents use the existing conversation and message tables, generation lifecycle, streaming events, message renderer, citations, bookmarks, retry, regeneration, and cancellation. `ConversationSnapshotProvider` lets the drawer render another transcript through that same controller without changing the globally selected conversation. It does not create a second controller or stream listener lifecycle.

## Persistence

The conversation row has three additional fields:

- `tangent_parent_id`: the owning conversation. Non-null rows stay out of the ordinary lists, search results, and Explorer thread counts.
- `tangent_selection`: the selected passage as it was captured.
- `tangent_context_message_count`: the number of inherited messages, which the tangent drawer keeps out of its visible discussion.

Creation uses the same transaction as conversation branching. It copies the transcript through the selected answer, the model, system prompt, space, Explorer folder, linked documents, and web sources. The copied answer carries the selection as quoted conversational context. Neither the source transcript nor its update timestamp changes. An invalid, foreign, or unfinished anchor rolls back without creating a conversation.

Tangents created from another tangent stay in the original conversation's collection; the existing fork lineage records their immediate source. Deleting an anchor leaves the captured passage intact. Deleting the parent cascades to its tangents. Explorer folder removal lists only top-level conversations so it does not attempt to delete their already-cascaded children a second time.

**Make conversation** clears the parent relationship on the same row. It keeps the transcript, inherited context, sources, space, and Explorer binding. The promoted conversation appears in ordinary lists and survives deletion of its former parent.

The conversation detail response includes `tangentParentId`. Selecting a tangent by a saved message link opens its parent and requests the tangent drawer. The controller never inserts that detail into the main conversation list.

## Verification

- Repository tests in `src-tauri/src/features/conversation/repository/tests/tangents.rs` cover ownership, snapshot isolation, hidden lists, invalid anchors, transaction rollback, promotion, deletion, nested tangents, and Explorer context.
- The selection and reply-action tests exercise discovery after highlighting, preserved focus, selection boundaries, keyboard behavior, and narrowing long replies. Controller tests cover cache isolation, composer recovery, and linked tangents.
- The `chat tangents` journeys in `e2e/app-shell.spec.ts` use the production renderer in Chromium and WebKit at wide and narrow widths. They exercise all three entry points, sending, independent drafts, reopening saved tangents, deep links, promotion failure and retry, and the main transcript remaining intact.

Browser fixtures model the IPC responses; the native repository tests verify actual SQLite behavior.
