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

export interface SnapshotMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  createdAt: string;
  metadata?: string | null;
}

export interface ConversationSnapshot {
  id: string;
  conversationId: string;
  conversationTitle: string;
  capturedAt: string;
  messageCount: number;
  messages: SnapshotMessage[];
}

export interface WorkspaceNote {
  id: string;
  title: string;
  content: string;
  linkedDocumentIds: string[];
  linkedConversationIds: string[];
  highlights: NoteHighlight[];
  stickyNotes: StickyItem[];
  conversationSnapshots: ConversationSnapshot[];
  createdAt: string;
  updatedAt: string;
}

export interface ListWorkspaceNotesResponse {
  notes: WorkspaceNote[];
}
