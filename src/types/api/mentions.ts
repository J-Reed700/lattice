/**
 * Mentions API Types
 *
 * Type definitions for @mentions and wikilinks.
 * These types match the Rust backend structures from commands/mentions.rs and repositories/mention_repository.rs
 */

export type {
  BacklinksResultDto,
  ExtractMentionsResultDto,
  GetMentionsForDocumentResultDto,
  MentionDto,
  MentionWithContextDto,
  SearchMentionsResultDto,
} from '../../lib/bindings';

export type Mention = import('../../lib/bindings').MentionDto;

export type MentionWithContext = import('../../lib/bindings').MentionWithContextDto;

export type ExtractMentionsResponse = import('../../lib/bindings').ExtractMentionsResultDto;

export type SearchMentionsResponse = import('../../lib/bindings').SearchMentionsResultDto;

export type BacklinksResponse = import('../../lib/bindings').BacklinksResultDto;

/** `get_mentions_for_document` returns the whole result object, not a bare array. */
export type GetMentionsForDocumentResult = import('../../lib/bindings').GetMentionsForDocumentResultDto;
