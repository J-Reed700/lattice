/**
 * Mentions API Types
 *
 * Type definitions for @mentions and wikilinks.
 * These types match the Rust backend structures from commands/mentions.rs and repositories/mention_repository.rs
 */

export type Mention = import('../../lib/bindings').MentionDto;

export type MentionWithContext = import('../../lib/bindings').MentionWithContextDto;

export interface ExtractMentionsResponse {
  mentions: MentionWithContext[];
  count: number;
}

export type SearchMentionsResponse = import('../../lib/bindings').SearchMentionsResultDto;

export interface BacklinksResponse {
  documentIds: string[];
}

/** `get_mentions_for_document` returns the whole result object, not a bare array. */
export interface GetMentionsForDocumentResult {
  documentId: string;
  mentions: MentionWithContext[];
  count: number;
}
