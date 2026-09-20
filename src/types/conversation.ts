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
  pageNumber?: number;
  chunkExcerpts?: SourceChunkExcerpt[];
  score: number;
  fileSizeBytes: number;
  modifiedAt: string;
  /**
   * The number the model was told to cite this source as — the `n` in `[n]`.
   *
   * Resolve footnotes by this value, never by array position: the backend's
   * prompt and this list are built by different pipelines, so position only
   * happened to agree when every document contributed exactly one chunk.
   */
  citationId?: number;
}

export interface SourceChunkExcerpt {
  chunkId: string;
  excerpt: string;
  section?: string;
  chunkIndex?: number;
  pageNumber?: number;
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
  section: z.string().max(1000).optional(),
  chunkIndex: z.number().min(0).max(1_000_000).optional(),
  pageNumber: z.number().int().min(1).optional(),
  chunkExcerpts: z.array(z.object({
    chunkId: z.string().min(1).max(255),
    excerpt: z.string().min(1).max(4000),
    section: z.string().max(1000).optional(),
    chunkIndex: z.number().min(0).max(1_000_000).optional(),
  pageNumber: z.number().int().min(1).optional(),
    score: z.number().finite().min(0),
    highlights: z.array(z.string().max(64)).max(12).optional(),
  })).max(24).optional(),
  score: z.number().finite().min(0),
  fileSizeBytes: z.number().min(0),
  modifiedAt: z.string(), // ISO 8601
  citationId: z.number().int().min(1).max(1000).optional(),
}).strict(); // Reject unknown properties

/**
 * Validate array of sources with max limit to prevent DoS
 */
export const SourcesArraySchema = z.array(SourceWithMetadataSchema)
  .max(100); // Limit max sources to prevent DoS

/**
 * One claim and how it was checked.
 *
 * `method` says which pass produced the verdict: `lexical` is stemmed token
 * overlap against the cited passage, `judge` is an LLM entailment decision.
 * Treat them as different strengths of evidence, not the same verdict.
 */
export const ClaimVerdictSchema = z.object({
  sentence: z.string().max(2000),
  citationIds: z.array(z.number().int().min(1).max(1000)).max(24),
  verdict: z.enum(['supported', 'contradicted', 'unsupported']),
  evidenceQuote: z.string().max(1000).nullable().optional(),
  method: z.enum(['lexical', 'judge']),
}).strict();

export type ClaimVerdict = z.infer<typeof ClaimVerdictSchema>;

export const MessageVerificationSummarySchema = z.object({
  enabled: z.boolean(),
  claimsEvaluated: z.number().min(0).optional(),
  supportedClaims: z.number().min(0).optional(),
  supportedClaimNotes: z.array(z.string().max(2000)).max(40).optional(),
  unsupportedClaims: z.array(z.string().max(2000)).max(20).optional(),
  groundedRatio: z.number().min(0).max(1).optional(),
  // Added alongside the LLM claim judge. Absent on messages verified before it
  // existed, so every consumer must tolerate `undefined` rather than read a
  // missing count as zero.
  /** Claims a passage actively refutes. Also present in `unsupportedClaims`. */
  contradictedClaims: z.array(z.string().max(2000)).max(20).optional(),
  verdictCounts: z.object({
    supported: z.number().min(0),
    contradicted: z.number().min(0),
    unsupported: z.number().min(0),
  }).strict().optional(),
  claimVerdicts: z.array(ClaimVerdictSchema).max(60).optional(),
  /** Whether any claim in this turn was judged by a model rather than by overlap. */
  judgeUsed: z.boolean().optional(),
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
  /**
   * Documents this chat is pinned to; `[]` or absent means the whole space.
   *
   * The backend intersects these with the conversation's space scope before
   * anything reads them, so naming a document from another space narrows the
   * turn to nothing rather than reaching outside. A request can only ever
   * narrow what a chat may read.
   */
  focusDocumentIds?: string[];
}

/**
 * One document a chat in a space may read — what `@` offers in the composer.
 *
 * Comes from the same allow-list retrieval derives its scope from, so the
 * picker can never name a file the turn would then fail to search.
 */
export interface SpaceDocument {
  documentId: string;
  fileName: string;
  category: string | null;
  modifiedAt: string | null;
}

/**
 * A web page as the app read it: the article text the model was given, not the
 * search engine's one-line snippet.
 *
 * Served from the page cache, so opening a cited article costs no request when
 * the turn that cited it already read it.
 */
export interface WebPage {
  /** The URL after redirects. */
  url: string;
  title: string | null;
  /** Extracted article text, paragraphs separated by blank lines. */
  text: string;
  wordCount: number;
  /** RFC 3339. When the text was actually fetched, not when it was served. */
  fetchedAt: string;
  fromCache: boolean;
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
  /** Active compaction summary, if older messages were folded into a summary. */
  compaction?: CompactionRecord | null;
  /**
   * The conversation this one was branched from, when it was.
   *
   * No foreign key stands behind it, so the parent may since have been
   * deleted. A reader that cannot find it shows no lineage at all — a
   * dangling id is an ordinary state, not an error.
   */
  forkedFromConversationId?: string | null;
  /** The parent turn the branch was taken at, when one was named. */
  forkedFromMessageId?: string | null;
  messages?: ConversationMessage[];
}

export interface ConversationMessage {
  id: string;
  conversationId: string;
  role: string;
  content: string;
  tokens: number;
  createdAt: string;
  status: MessageStatus;
  metadata?: string | null;
  sources?: SourceWithMetadata[];
}

/**
 * A compaction record: the oldest messages of a conversation folded into an
 * LLM summary. The original messages stay in the history for display; only
 * the LLM context switches to the summary from here on.
 */
export interface CompactionRecord {
  id: string;
  conversationId: string;
  summaryText: string;
  /** Id of the last message folded into the summary (inclusive boundary). */
  upToMessageId: string;
  originalMessageCount: number;
  originalTokens: number;
  summaryTokens: number;
  /** `summaryTokens / originalTokens`, clamped to (0, 1]. */
  compressionRatio: number;
  createdAt: string;
}

export interface ConversationMessageBookmark {
  id: string;
  conversationId: string;
  conversationTitle: string;
  spaceId: string;
  messageId: string;
  messageRole: string;
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

/**
 * Retrieval trace for one assistant turn.
 *
 * `searchedDocuments` is the size of the set the backend's hard space-scope
 * filter actually allowed — not the size of the corpus. An absent trace means
 * retrieval did not run; it never means zeros.
 */
export interface RetrievalTrace {
  searchedDocuments: number;
  /** Passages from documents in the vault. Web results are not counted here. */
  passages: number;
  /** Documents in the vault. Web results are `webPages`, never files. */
  files: number;
  /**
   * Web pages carried into the answer.
   *
   * Counted apart from `files` because they are not the user's documents:
   * folding them in made a web-only answer in an empty space report that it had
   * read ten files, which reads as a scope leak rather than a working search.
   */
  webPages?: number;
  scope: 'vault' | 'linked';
  /** Why the knowledge base could not be searched, straight from the backend. */
  unavailableReason?: string;
  // Added alongside the post-rerank sufficiency check. Absent on every trace
  // persisted before it existed, so an absent verdict means "not judged" and
  // never "insufficient".
  /** The post-rerank sufficiency verdict, after any corrective pass. */
  kbSufficient?: boolean;
  /** Replanned retrieval passes that ran. Capped at one per turn. */
  kbCorrectiveRetries?: number;
  /** True when a short follow-up reused the last topic and skipped the planner. */
  kbPlannerSkipped?: boolean;
  /** Stable verdict codes (`low_term_coverage`, …), not prose. */
  sufficiencyReasons?: string[];
  /**
   * How many documents this turn was pinned to, when it was pinned at all.
   *
   * The count *after* the intersection with the space scope, so `0` means the
   * request named documents this chat cannot reach and the turn searched
   * nothing — never that it fell back to the whole space.
   */
  focusedDocuments?: number;
}

export const RetrievalTraceSchema = z.object({
  searchedDocuments: z.number().int().min(0).max(100_000_000),
  passages: z.number().int().min(0).max(1000),
  files: z.number().int().min(0).max(1000),
  webPages: z.number().int().min(0).max(1000).optional(),
  scope: z.enum(['vault', 'linked']),
  unavailableReason: z.string().max(400).optional(),
  kbSufficient: z.boolean().optional(),
  kbCorrectiveRetries: z.number().int().min(0).max(8).optional(),
  kbPlannerSkipped: z.boolean().optional(),
  sufficiencyReasons: z.array(z.string().max(64)).max(8).optional(),
  focusedDocuments: z.number().int().min(0).max(1000).optional(),
}).strict();

/**
 * What a turn did, step by step.
 *
 * The same structure is streamed while the turn runs and persisted when it
 * ends, so the timeline under a finished answer is the list the reader watched
 * tick past. An absent record means "not recorded" — every answer written
 * before this existed has none — and never an empty turn.
 */
export const TurnStepKindSchema = z.enum([
  /** Not work: the turn was asked to research. See `TurnStepKind::DeepResearch`. */
  'deep_research',
  'route',
  'plan',
  'search_documents',
  'sufficiency',
  'corrective_search',
  'web_search',
  'read_page',
  'wiki',
  'open_document',
  'tool',
  'generate',
  'verify',
  'retry',
]);

export type TurnStepKind = z.infer<typeof TurnStepKindSchema>;

export const TurnStepLinkSchema = z.object({
  url: z.string().min(1).max(2048),
  title: z.string().max(400).nullable().optional(),
}).strict();

export type TurnStepLink = z.infer<typeof TurnStepLinkSchema>;

export const TurnStepSchema = z.object({
  /** Stable within the turn; a finish event carries the id of its start. */
  id: z.string().min(1).max(64),
  kind: TurnStepKindSchema,
  /** A human sentence, as the activity labels were: "Searching your documents". */
  label: z.string().max(200),
  /** The query, the host, a tool's argument summary. */
  detail: z.string().max(400).nullable().optional(),
  state: z.enum(['running', 'done', 'failed']),
  /** Offset from the start of the turn, not a wall clock. */
  startedAtMs: z.number().int().min(0),
  /** Absent while the step is still running. Absent is not zero. */
  durationMs: z.number().int().min(0).nullable().optional(),
  /** What it produced: "8 passages from 3 files", "not enough support: …". */
  result: z.string().max(400).nullable().optional(),
  /**
   * For a search, the pages it found; for a page read, the page. A read knows
   * its address from the start and a search only at the end, so either event
   * may carry it. Absent on every step that went nowhere on the web.
   */
  links: z.array(TurnStepLinkSchema).max(12).optional(),
}).strict();

export type TurnStep = z.infer<typeof TurnStepSchema>;

export const TurnRecordSchema = z.object({
  /** The model that answered **this** turn, which the conversation's may not be. */
  model: z.object({
    id: z.string().max(400),
    name: z.string().max(400),
  }).strict().nullable(),
  steps: z.array(TurnStepSchema).max(200),
  timing: z.object({
    /** Time to the persisted answer, not to the last token. */
    totalMs: z.number().int().min(0),
    routerMs: z.number().int().min(0),
    retrievalMs: z.number().int().min(0),
    generationMs: z.number().int().min(0),
    verificationMs: z.number().int().min(0),
    /** Time inside tool calls — part of `generationMs`, not beside it. */
    toolMs: z.number().int().min(0),
  }).strict(),
  tokens: z.object({
    completion: z.number().int().min(0).nullable(),
    contextUsed: z.number().int().min(0).nullable(),
  }).strict(),
  /**
   * What the router decided and how sure it was. `null` on every turn where no
   * router ran — a forced web search, a closed-book turn, a chat with nothing
   * to route.
   */
  router: z.object({
    action: z.string().max(64),
    confidence: z.number().min(0).max(1),
    rationale: z.string().max(4000).nullable(),
  }).strict().nullable(),
}).strict();

export type TurnRecord = z.infer<typeof TurnRecordSchema>;

/** Where in a file a cited passage lives, and how to find it again. */
export interface PassageLocator {
  /** The chunk text as sent in the citation payload. */
  text: string;
  /** Ordinal of the chunk in the document, when known. */
  chunkIndex?: number;
  pageNumber?: number;
  /** 1-based PDF page, only when already resolved. */
  page?: number;
  /** Terms to sub-highlight inside the located passage. */
  highlights?: string[];
  /** Display label: "p. 12", "12:40–13:05", "§ Methods". */
  label?: string;
  /** Identity for the resolved-location memo. */
  chunkId?: string;
}

/**
 * How confidently a passage was located in the open file.
 *
 * `approximate` is shown to the user as such — a scroll position derived from
 * the chunk ordinal is a guess, and the UI says so rather than implying the
 * highlight is where the passage really is.
 */
export type PassageMatchTier = 'exact' | 'approximate' | 'none';
