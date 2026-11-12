/**
 * Daily Notes API Types
 *
 * Types for daily notes feature (work in progress).
 */

/**
 * Daily note representation
 * TODO: Define proper fields when backend is implemented
 */
export type DailyNote = import('../../lib/bindings').DailyNoteCompatDto;

export type NoteHighlight = import('../../lib/bindings').NoteHighlightDto;

export type SnapshotMessage = import('../../lib/bindings').SnapshotMessageDto;

export type ConversationSnapshot = import('../../lib/bindings').ConversationSnapshotDto;

export type WorkspaceNote = import('../../lib/bindings').WorkspaceNoteDto;

export type ListWorkspaceNotesResponse = import('../../lib/bindings').ListWorkspaceNotesResponseDto;
