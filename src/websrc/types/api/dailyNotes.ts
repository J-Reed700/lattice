/**
 * Daily Notes API Types
 *
 * Types for daily notes feature (work in progress).
 */

/**
 * Daily note representation
 * TODO: Define proper fields when backend is implemented
 */
export interface DailyNote {
  id: string;
  date: string;
  content: string;
  createdAt: string;
  updatedAt: string;
}

export interface NoteHighlight {
  id: string;
  text: string;
  createdAt: string;
}

export interface StickyItem {
  id: string;
  text: string;
  color: string;
  createdAt: string;
}

export type SnapshotMessage = import('../../lib/bindings').SnapshotMessageDto;

export type ConversationSnapshot = import('../../lib/bindings').ConversationSnapshotDto;

export type WorkspaceNote = import('../../lib/bindings').WorkspaceNoteDto;

export type ListWorkspaceNotesResponse = import('../../lib/bindings').ListWorkspaceNotesResponseDto;
