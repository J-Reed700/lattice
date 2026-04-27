/**
 * Conversation Types
 *
 * Type definitions for conversational LLM interactions
 */

import { z } from 'zod';

import type { SearchResult } from './searchResult';

/**
 * SourceWithMetadata - Rich source metadata for citations
 *
 * Contains all metadata needed to render citation tooltips and
 * enable click-to-view functionality.
 */
export interface SourceWithMetadata {
  documentId: string;
  chunkId: string;
  fileName: string;
  filePath: string;
  mimeType: string;
  category: string;
  content: string;
  excerpt?: string;
  highlights?: string[];
  section?: string;
  chunkIndex?: number;
  chunkExcerpts?: SourceChunkExcerpt[];
  score: number;
  fileSizeBytes: number;
  modifiedAt: string;
}

export interface SourceChunkExcerpt {
  chunkId: string;
  excerpt: string;
  section?: string;
  chunkIndex?: number;
  score: number;
  highlights?: string[];
}

/**
 * Zod validation schema for SourceWithMetadata
 * Validates JSON-deserialized metadata to prevent state corruption and XSS
 */
export const SourceWithMetadataSchema = z.object({
  documentId: z.string().min(1).max(255),
  chunkId: z.string().min(1).max(255),
  fileName: z.string().min(1).max(512),
  filePath: z.string().min(1).max(2048),
  mimeType: z.string().min(1).max(255),
  category: z.string().min(1).max(100),
  content: z.string().max(10000), // Limit preview content
  excerpt: z.string().max(2000).optional(),
  highlights: z.array(z.string().max(64)).max(12).optional(),
  section: z.string().max(200).optional(),
  chunkIndex: z.number().min(0).max(1_000_000).optional(),
  chunkExcerpts: z.array(z.object({
    chunkId: z.string().min(1).max(255),
    excerpt: z.string().min(1).max(4000),
    section: z.string().max(200).optional(),
    chunkIndex: z.number().min(0).max(1_000_000).optional(),
    score: z.number().finite().min(0),
    highlights: z.array(z.string().max(64)).max(12).optional(),
  })).max(24).optional(),
  score: z.number().finite().min(0),
  fileSizeBytes: z.number().min(0),
  modifiedAt: z.string(), // ISO 8601
}).strict(); // Reject unknown properties

/**
 * Validate array of sources with max limit to prevent DoS
 */
export const SourcesArraySchema = z.array(SourceWithMetadataSchema)
  .max(100); // Limit max sources to prevent DoS

export const MessageVerificationSummarySchema = z.object({
  enabled: z.boolean(),
  claimsEvaluated: z.number().min(0).optional(),
  supportedClaims: z.number().min(0).optional(),
  supportedClaimNotes: z.array(z.string().max(2000)).max(40).optional(),
  unsupportedClaims: z.array(z.string().max(2000)).max(20).optional(),
  groundedRatio: z.number().min(0).max(1).optional(),
}).strict();

export type MessageVerificationSummary = z.infer<typeof MessageVerificationSummarySchema>;

/**
 * Type inference from validated schema
 */
export type ValidatedSources = z.infer<typeof SourcesArraySchema>;

/**
 * Message status for optimistic UI
 */
export type MessageStatus = 'pending' | 'processing' | 'completed' | 'failed';

export type TurnMode = 'auto' | 'followup' | 'query';

export interface ToolPreferences {
  knowledgeBase: boolean;
  webSearch: boolean;
  deepResearchMode?: boolean;
  followupMode?: boolean;
  turnMode?: TurnMode;
  enabledTools?: string[];
}

export interface Conversation {
  id: string;
  title: string;
  modelName?: string;
  systemPrompt?: string | null;
  spaceId?: string | null;
  isSaved?: boolean | null;
  isBookmarked?: boolean | null;
  isPinned?: boolean | null;
  isArchived?: boolean | null;
  savedAt?: string | null;
  bookmarkedAt?: string | null;
  pinnedAt?: string | null;
  archivedAt?: string | null;
  lastMessagePreview?: string | null;
  createdAt?: string;
  updatedAt: string;
  messageCount?: number;
  totalTokens?: number;
  messages?: ConversationMessage[];
}

export interface ConversationMessage {
  id: string;
  conversationId: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  tokens: number;
  createdAt: string;
  status: MessageStatus;
  metadata?: string | null;
  sources?: SourceWithMetadata[];
}

export interface ConversationMessageBookmark {
  id: string;
  conversationId: string;
  conversationTitle: string;
  spaceId: string;
  messageId: string;
  messageRole: 'user' | 'assistant' | 'system';
  messagePreview: string;
  title?: string | null;
  note?: string | null;
  createdAt: string;
}

export interface ConversationalAnswer {
  answer: string;
  sources: SearchResult[];
  conversationId: string;
  messageCount: number;
  totalTokens: number;
}

export interface CreateConversationParams {
  title: string;
  modelName: string;
  systemPrompt?: string | null;
}

export interface AskQuestionParams {
  question: string;
  maxResults?: number;
}

export interface StreamChunk {
  content: string;
  isComplete: boolean;
}

/**
 * Optimistic message for immediate UI feedback
 *
 * Temporary message shown to user while backend processes the request.
 * Replaced by real ConversationMessage when backend responds.
 */
export interface OptimisticMessage {
  /** Temporary ID (crypto.randomUUID()) */
  tempId: string;
  /** Conversation ID this message belongs to */
  conversationId: string;
  /** Message content */
  content: string;
  /** Message role */
  role: 'user' | 'assistant';
  /** Current status */
  status: MessageStatus;
  /** Creation timestamp */
  createdAt: string;
  /** Error message if status='failed' */
  error?: string;
}

/**
 * Type guard to check if message is optimistic (temporary)
 */
export function isOptimistic(message: DisplayMessage): message is OptimisticMessage {
  return 'tempId' in message;
}

/**
 * Union type for displaying messages (real or optimistic)
 */
export type DisplayMessage = ConversationMessage | OptimisticMessage;
