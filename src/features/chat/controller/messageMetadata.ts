import type {
  ConversationMessage,
  MessageVerificationSummary,
  RetrievalTrace,
  SourceWithMetadata,
  TurnRecord,
  TurnStep,
} from '@/types/conversation';
import {
  MessageVerificationSummarySchema,
  RetrievalTraceSchema,
  SourceWithMetadataSchema,
  SourcesArraySchema,
  TurnRecordSchema,
} from '@/types/conversation';

export const normalizeChunkExcerpt = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const excerpt = String(value.excerpt ?? '').trim();
  if (!chunkId || !excerpt) return null;
  const rawScore = Number(value.score ?? 0);
  return {
    chunkId,
    excerpt,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    pageNumber: value.pageNumber ?? value.page_number ?? undefined,
    score: Number.isFinite(rawScore) && rawScore >= 0 ? rawScore : 0,
    highlights: value.highlights ?? undefined,
  };
};

export const normalizeSource = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const filePath = String(value.filePath ?? value.file_path ?? '').trim();
  const fileName = String(value.fileName ?? value.file_name ?? '').trim();
  const documentId = String(value.documentId ?? value.document_id ?? '').trim();
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const resolvedPath = filePath || 'unknown://source';
  const resolvedDocumentId = documentId || `source:${resolvedPath}`;
  const rawScore = Number(value.score ?? 0);
  const rawExcerpts = Array.isArray(value.chunkExcerpts)
    ? value.chunkExcerpts
    : (Array.isArray(value.chunk_excerpts) ? value.chunk_excerpts : []);
  return {
    documentId: resolvedDocumentId,
    chunkId: chunkId || `${resolvedDocumentId}#chunk`,
    fileName: fileName || resolvedPath,
    filePath: resolvedPath,
    mimeType: value.mimeType ?? value.mime_type ?? 'text/plain',
    category: value.category ?? (resolvedPath.startsWith('http') ? 'Web Article' : 'Unknown'),
    content: String(value.content ?? '').trim(),
    excerpt: value.excerpt ?? undefined,
    highlights: value.highlights ?? undefined,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    pageNumber: value.pageNumber ?? value.page_number ?? undefined,
    chunkExcerpts: rawExcerpts
      .map(normalizeChunkExcerpt)
      .filter((item): item is Record<string, unknown> => item !== null),
    score: Number.isFinite(rawScore) && rawScore >= 0 ? rawScore : 0,
    fileSizeBytes: value.fileSizeBytes ?? value.file_size_bytes ?? 0,
    modifiedAt: value.modifiedAt ?? value.modified_at ?? '',
    citationId: value.citationId ?? value.citation_id ?? undefined,
    webSnapshot: value.webSnapshot ?? value.web_snapshot ?? undefined,
  };
};

export const parseSources = (raw: unknown): SourceWithMetadata[] => {
  if (!Array.isArray(raw)) return [];
  const normalized = raw
    .map(normalizeSource)
    .filter((source): source is Record<string, unknown> => source !== null);
  const parsedArray = SourcesArraySchema.safeParse(normalized);
  if (parsedArray.success) return parsedArray.data;
  return normalized.flatMap(source => {
    const parsed = SourceWithMetadataSchema.safeParse(source);
    return parsed.success ? [parsed.data] : [];
  });
};

export const parseVerification = (raw: unknown): MessageVerificationSummary | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const parsed = MessageVerificationSummarySchema.safeParse({
    enabled: Boolean(value.enabled),
    pending: value.pending,
    interrupted: value.interrupted,
    claimsEvaluated: value.claimsEvaluated ?? value.claims_evaluated,
    supportedClaims: value.supportedClaims ?? value.supported_claims,
    supportedClaimNotes: value.supportedClaimNotes ?? value.supported_claim_notes,
    unsupportedClaims: value.unsupportedClaims ?? value.unsupported_claims,
    groundedRatio: value.groundedRatio ?? value.grounded_ratio,
    contradictedClaims: value.contradictedClaims ?? value.contradicted_claims,
    verdictCounts: value.verdictCounts ?? value.verdict_counts,
    claimVerdicts: value.claimVerdicts ?? value.claim_verdicts,
    judgeUsed: value.judgeUsed ?? value.judge_used,
  });
  return parsed.success ? parsed.data : null;
};

/** A grounding check that finished after its turn returned. */
export interface VerificationPatch {
  messageId: string;
  verification: unknown;
  turn?: unknown;
}

/**
 * How long a turn keeps listening for its answer's background check. The
 * judge's own budget is ninety seconds; loading the utility model and reading
 * the cited pages come on top. Past this the badge stays on "checking" until
 * the next load reads the persisted result.
 */
export const VERIFICATION_WAIT_MS = 5 * 60 * 1000;

/** Write a finished check into a message's metadata, as the backend persisted it. */
export const applyVerificationPatch = (
  messages: ConversationMessage[],
  patch: VerificationPatch
): ConversationMessage[] => messages.map(message => {
  if (message.id !== patch.messageId) return message;
  let metadata: Record<string, unknown> = {};
  if (message.metadata) {
    try {
      metadata = JSON.parse(message.metadata) as Record<string, unknown>;
    } catch {
      metadata = {};
    }
  }
  metadata.verification = patch.verification;
  if (patch.turn) metadata.turn = patch.turn;
  return { ...message, metadata: JSON.stringify(metadata) };
});

/** Every message still waiting on its check is marked as never having got it. */
export const markVerificationInterrupted = (messages: ConversationMessage[]): ConversationMessage[] =>
  messages.map(message => {
    if (!isVerificationPending(message) || !message.metadata) return message;
    try {
      const metadata = JSON.parse(message.metadata) as Record<string, unknown>;
      const verification = metadata.verification as Record<string, unknown>;
      metadata.verification = { ...verification, pending: false, interrupted: true };
      return { ...message, metadata: JSON.stringify(metadata) };
    } catch {
      return message;
    }
  });

/** Whether a message was persisted with its check still running. */
export const isVerificationPending = (message: ConversationMessage | undefined): boolean => {
  if (!message?.metadata) return false;
  try {
    const metadata = JSON.parse(message.metadata) as { verification?: { pending?: unknown } };
    return metadata.verification?.pending === true;
  } catch {
    return false;
  }
};

/**
 * A persisted retrieval trace, or null.
 *
 * Absent means "no trace" — messages written before this feature, and turns
 * where retrieval never ran, have no key. Never read that as zeros.
 */
export const parseRetrievalTrace = (raw: unknown): RetrievalTrace | null => {
  if (!raw || typeof raw !== 'object') return null;
  const parsed = RetrievalTraceSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
};

/**
 * A persisted turn record, or null.
 *
 * Absent for every answer written before the record existed, and for any turn
 * that recorded nothing. Never read that as a turn that did nothing.
 */
export const parseTurnRecord = (raw: unknown): TurnRecord | null => {
  if (!raw || typeof raw !== 'object') return null;
  const parsed = TurnRecordSchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
};

export const deriveMessageMetadata = (messages: ConversationMessage[]) => {
  const sources = new Map<string, SourceWithMetadata[]>();
  const verification = new Map<string, MessageVerificationSummary>();
  const retrieval = new Map<string, RetrievalTrace>();
  const turn = new Map<string, TurnRecord>();
  for (const message of messages) {
    const directSources = parseSources(message.sources);
    if (directSources.length > 0) sources.set(message.id, directSources);
    if (!message.metadata) continue;
    try {
      const metadata = JSON.parse(message.metadata) as Record<string, unknown>;
      const persistedSources = parseSources(metadata.sources);
      if (persistedSources.length > 0) sources.set(message.id, persistedSources);
      const persistedVerification = parseVerification(metadata.verification);
      if (persistedVerification) verification.set(message.id, persistedVerification);
      const persistedRetrieval = parseRetrievalTrace(metadata.retrieval);
      if (persistedRetrieval) retrieval.set(message.id, persistedRetrieval);
      const persistedTurn = parseTurnRecord(metadata.turn);
      if (persistedTurn) turn.set(message.id, persistedTurn);
    } catch {
      // A malformed metadata field must not hide the message itself.
    }
  }
  return { sources, verification, retrieval, turn };
};

/**
 * Merge one step event into a conversation's live list.
 *
 * A finish event carries the id of its start, so it replaces that step where it
 * already is. Appending instead would make a finished step jump to the bottom
 * of the timeline the moment it completed.
 */
export const mergeStep = (existing: TurnStep[] | undefined, step: TurnStep): TurnStep[] => {
  const steps = existing ?? [];
  const index = steps.findIndex(candidate => candidate.id === step.id);
  if (index === -1) return [...steps, step];
  const next = [...steps];
  next[index] = step;
  return next;
};
