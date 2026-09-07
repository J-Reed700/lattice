/**
 * Mentions API Types
 *
 * Type definitions for @mentions and wikilinks.
 * These types match the Rust backend structures from commands/mentions.rs and repositories/mention_repository.rs
 */

export interface Mention {
  id: string;
  name: string;
  mentionType: string;    // Matches Rust mention_type with camelCase serialization
  metadata: string | null;
  created_at: string;
}

export interface MentionWithContext {
  id: string;
  name: string;
  mentionType: string;
  documentId: string;
  context: string;        // Flattened to match Rust MentionWithContextDto
  position: number;
  createdAt: string;
}

export interface ExtractMentionsResponse {
  mentions: MentionWithContext[];
  count: number;
}

export interface SearchMentionsResponse {
  mentions: Mention[];
}

export interface BacklinksResponse {
  documentIds: string[];
}

/** `get_mentions_for_document` returns the whole result object, not a bare array. */
export interface GetMentionsForDocumentResult {
  documentId: string;
  mentions: MentionWithContext[];
  count: number;
}
