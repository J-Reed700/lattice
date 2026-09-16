# Chat + Daily Notes + Reference Inbox Consolidation Plan

## Scope
This document audits overlap between Chat, Daily Notes, and Reference Inbox, then proposes a reduced surface area with clearer ownership.

## Current Surface Inventory

| Surface | Primary intent | Current actions | Overlap risk |
| --- | --- | --- | --- |
| `src/components/Chat/MessageBubble.tsx` | Per-message quick actions | Bookmark to Snippets, Save to Daily Notes, Copy | Duplicates capture with Sidebar and Reference Inbox |
| `src/components/Chat/ConversationSidebar.tsx` | Conversation navigation + snippet list | Snippet filters, edit title/note, capture/re-capture, open capture, open inbox | Heavy overlap with Reference Inbox detail and batch triage |
| `src/components/ReferenceInbox/ReferenceInbox.tsx` | Triage and process references | Search/filter, annotate, capture one, capture pending, open in chat, open captured note, copy | Strongly overlaps with Sidebar snippet details |
| `src/components/DailyNotes/DailyNotesWorkspace.tsx` | Note authoring + snapshots | Capture active chat, capture selected chat, snapshot insert, open in chat | Overlaps with message-level capture mental model |
| `src/components/Chat/ChatPanel.tsx` | Ask/answer composer | Turn mode, KB/Web/Wiki/Deep/custom tool controls | Not data overlap, but contributes visual crowding |

## Findings

1. Two places currently behave like "reference detail editors":  
`src/components/Chat/ConversationSidebar.tsx` and `src/components/ReferenceInbox/ReferenceInbox.tsx`.
2. Three separate capture entry points exist for different granularity without clear hierarchy:
message capture, snippet capture, and full-conversation snapshot capture.
3. Language is fragmented (`Bookmark`, `Snippet`, `Capture`, `Reference`, `Snapshot`) for similar user goals.
4. Chat sidebar has high interaction density while also serving conversation navigation.

## Proposed Ownership Model

1. `Chat message`: quick actions only.
2. `Reference Inbox`: single home for triage/edit/capture state of saved references.
3. `Daily Notes`: writing workspace and destination of captured output, plus full conversation snapshots.

## Keep / Merge / Remove

1. Keep in MessageBubble:
- `Bookmark` (save reference candidate)
- `Copy`
- Optional `Quick Capture` (single-click power user action)

2. Keep in Reference Inbox (source of truth):
- Annotation editing
- Capture one
- Capture pending
- Status filtering (`pending` / `captured`)
- Open in chat and open captured note

3. Simplify ConversationSidebar Snippets:
- Keep list, role filter, and `Open in Chat`
- Keep status chip (`Captured` / `Pending`)
- Remove inline annotation fields and capture buttons
- Keep one CTA: `Open Reference Inbox`

4. Keep in Daily Notes:
- Full conversation snapshot capture (`Capture Active Chat` / capture from chat list)
- Snapshot to note insertion and deep link back to chat
- Do not duplicate reference triage controls here

## Naming Consolidation

1. User-facing term: `Reference`
2. Internal/legacy label migration:
- `Snippet` -> `Reference`
- `Bookmark message to snippets` -> `Save as reference`
- `Capture` (message-level) -> `Capture to Daily Notes`
- `Snapshot` reserved only for full-conversation captures

## Implementation Sequence (Low Risk)

1. Phase 1 (UI declutter):
- Remove snippet detail editor block from `ConversationSidebar`.
- Keep only list + status + "Open Reference Inbox".

2. Phase 2 (label unification):
- Replace visible `Snippet` strings with `Reference`.
- Keep API/storage names as-is for compatibility.

3. Phase 3 (capture hierarchy):
- Ensure all per-message and per-snippet capture actions route through shared utility
  (`src/utils/chatReferenceCapture.ts`), with Inbox as the primary triage destination.

4. Phase 4 (composer density):
- Collapse tool controls behind one compact row by default in `ChatPanel`.

## Non-Goals

1. No schema migrations in this pass.
2. No removal of Daily Notes snapshot functionality.
3. No replacement of existing deep-link behavior between chat and notes.

## Decision Applied

Message-level `Quick Capture` is now removed from primary bubble actions.

1. Default message action is `Save as reference`.
2. Capture workflows are centralized in `Reference Inbox`.
