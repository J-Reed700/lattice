/**
 * Turning a conversation — or one answer, or one checked sentence — into
 * markdown someone can paste into a paper, a journal page or an email.
 *
 * An answer in Lattice is a checkable document: the claim, the passages behind
 * it and the verdict on each sentence. Carrying only the prose out would throw
 * away the part that makes it worth quoting, so every block written here keeps
 * the numbered sources and says what the verification found.
 *
 * Everything in this file is pure. The callers own the clipboard, the journal
 * and the toasts.
 */

import { formatSourceLocation } from '@/components/Reading/passageLocator';
import {
  MessageVerificationSummarySchema,
  SourceWithMetadataSchema,
  type ClaimVerdict,
  type MessageVerificationSummary,
  type SourceWithMetadata,
} from '@/types/conversation';
import { createCitationMap } from '@/utils/citations';
import { sanitizeFileName } from '@/utils/sanitize';

/** How many ungrounded sentences a verification line names before it counts. */
const MAX_NAMED_UNSUPPORTED = 3;

/** Longest ungrounded sentence quoted inline before it is cut. */
const MAX_UNSUPPORTED_CHARS = 140;

/** What a conversation with no title of its own is called. */
const UNTITLED_CONVERSATION = 'Untitled conversation';

/** The shape this module needs from a conversation; anything else is ignored. */
export interface ExportableConversation {
  title?: string | null;
  spaceId?: string | null;
  updatedAt?: string | null;
  createdAt?: string | null;
}

/** The shape this module needs from a message. Matches `ConversationMessage`. */
export interface ExportableMessage {
  id?: string;
  role: string;
  content: string;
  createdAt?: string;
  metadata?: string | null;
  sources?: SourceWithMetadata[] | null;
}

export interface ConversationExportOptions {
  /**
   * Sources by message id, as the store holds them (`lastMessageSources`).
   * Consulted before the message's own metadata, exactly as `Message.tsx` does,
   * so an export says what the reader was looking at.
   */
  sources?: ReadonlyMap<string, SourceWithMetadata[]>;
  /** Verification by message id (`messageVerification`), same precedence. */
  verification?: ReadonlyMap<string, MessageVerificationSummary>;
  /** The space this chat searches. The conversation itself carries only an id. */
  spaceName?: string | null;
  /** Injected by the tests; the header stamp would otherwise move every run. */
  now?: Date;
}

/**
 * A cited web page rather than a document in the vault.
 *
 * Two tests because two things produce sources: the backend prefixes a web
 * result's id with `web:`, and older payloads only carried the category.
 */
export function isWebSource(
  source: Pick<SourceWithMetadata, 'documentId' | 'category'>,
): boolean {
  return (
    source.documentId.startsWith('web:') ||
    (source.category ?? '').toLowerCase().includes('web article')
  );
}

/**
 * The distinct vault documents an answer rests on, in citation order.
 *
 * What "Compare sources" opens. Web pages are left out: Compare reads files,
 * and handing it a page id produces a row that can never be filled.
 */
export function vaultDocumentIds(sources: readonly SourceWithMetadata[]): string[] {
  const seen = new Set<string>();
  const ids: string[] = [];
  for (const source of sources) {
    if (isWebSource(source)) continue;
    const id = source.documentId?.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
  }
  return ids;
}

/** One line of text: a markdown line break inside a citation would split it. */
function oneLine(value: string): string {
  return value.replace(/\s*\n+\s*/g, ' ').trim();
}

function truncate(value: string, limit: number): string {
  return value.length > limit ? `${value.slice(0, limit - 1).trimEnd()}…` : value;
}

/** "Halvorsen 2024.pdf — PDF p. 14", or just the file name when it has no locator. */
export function sourceLine(source: SourceWithMetadata): string {
  const name = oneLine(sanitizeFileName(source.fileName || source.filePath || 'Untitled source'));
  if (isWebSource(source)) {
    const url = oneLine(source.filePath ?? '');
    return url && url !== name ? `${name} — ${url}` : name;
  }
  const location = formatSourceLocation(source);
  return location ? `${name} — ${location}` : name;
}

/** The numbered source list under an answer, keyed by the `[n]` the text uses. */
function sourcesBlock(sources: readonly SourceWithMetadata[]): string[] {
  if (sources.length === 0) return [];
  const numbered = Array.from(createCitationMap([...sources]).entries()).sort(
    ([a], [b]) => a - b,
  );
  if (numbered.length === 0) return [];
  return [
    '**Sources**',
    '',
    ...numbered.map(([number, source]) => `${number}. ${sourceLine(source)}`),
    '',
  ];
}

/**
 * What the sentence-level check came to, in one line.
 *
 * Silent when verification is off or had nothing to check: "0 of 0 sentences
 * backed" reads as a failure, and no check ran at all.
 */
export function verificationLine(
  summary: MessageVerificationSummary | null | undefined,
): string | null {
  if (!summary?.enabled) return null;
  const evaluated = summary.claimsEvaluated ?? 0;
  if (evaluated <= 0) return null;
  const unsupported = summary.unsupportedClaims ?? [];
  const backed = Math.max(0, evaluated - unsupported.length);
  const head = `${backed} of ${evaluated} checked ${
    evaluated === 1 ? 'sentence' : 'sentences'
  } backed`;
  if (unsupported.length === 0) return `${head}.`;
  const named = unsupported
    .slice(0, MAX_NAMED_UNSUPPORTED)
    .map((claim) => `“${truncate(oneLine(claim), MAX_UNSUPPORTED_CHARS)}”`);
  const rest = unsupported.length - named.length;
  const tail = rest > 0 ? `${named.join('; ')}; and ${rest} more` : named.join('; ');
  return `${head}; not found: ${tail}.`;
}

/** The model and wall time a turn cost, when the turn record says. */
export function turnLine(rawTurn: unknown): string | null {
  if (!rawTurn || typeof rawTurn !== 'object') return null;
  const turn = rawTurn as { model?: unknown; timing?: unknown };
  const model =
    turn.model && typeof turn.model === 'object'
      ? (turn.model as { id?: unknown; name?: unknown })
      : null;
  const name =
    typeof model?.name === 'string' && model.name.trim()
      ? model.name.trim()
      : typeof model?.id === 'string' && model.id.trim()
        ? model.id.trim()
        : null;
  const timing =
    turn.timing && typeof turn.timing === 'object'
      ? (turn.timing as { totalMs?: unknown })
      : null;
  const totalMs =
    typeof timing?.totalMs === 'number' && Number.isFinite(timing.totalMs) && timing.totalMs >= 0
      ? timing.totalMs
      : null;
  const parts = [
    name ? `Answered by ${name}` : null,
    totalMs === null ? null : `${(totalMs / 1000).toFixed(1)} s`,
  ].filter(Boolean);
  return parts.length > 0 ? `${parts.join(' · ')}.` : null;
}

/** Whatever the message's metadata blob holds, or an empty object. */
function readMetadata(message: ExportableMessage): Record<string, unknown> {
  if (!message.metadata) return {};
  try {
    const parsed: unknown = JSON.parse(message.metadata);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {};
  } catch {
    // A malformed blob must not cost the export the message itself.
    return {};
  }
}

/**
 * Sources persisted on a message.
 *
 * Validated one at a time rather than as an array: one source written by an
 * older backend should cost that entry a line, not cost the answer its whole
 * source list.
 */
function parseSources(raw: unknown): SourceWithMetadata[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) => {
    const parsed = SourceWithMetadataSchema.safeParse(entry);
    return parsed.success ? [parsed.data] : [];
  });
}

function parseVerification(raw: unknown): MessageVerificationSummary | null {
  const parsed = MessageVerificationSummarySchema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

/** Everything one answer carries with it, from whichever source of truth has it. */
function answerFacts(message: ExportableMessage, options: ConversationExportOptions) {
  const metadata = readMetadata(message);
  const fromStore = message.id ? options.sources?.get(message.id) : undefined;
  const sources =
    message.sources && message.sources.length > 0
      ? message.sources
      : fromStore && fromStore.length > 0
        ? fromStore
        : parseSources(metadata.sources);
  const verification =
    (message.id ? options.verification?.get(message.id) : undefined) ??
    parseVerification(metadata.verification);
  return { sources, verification, turn: metadata.turn };
}

function roleHeading(role: string): string {
  if (role === 'user') return 'You';
  if (role === 'assistant') return 'Assistant';
  return role.charAt(0).toUpperCase() + role.slice(1);
}

function stamp(value: string | null | undefined, fallback: Date): string {
  const parsed = value ? new Date(value) : fallback;
  return Number.isNaN(parsed.getTime()) ? fallback.toLocaleString() : parsed.toLocaleString();
}

/**
 * The whole conversation as markdown: the question, the answer, the passages
 * behind it and what the check found, turn after turn.
 */
export function conversationToMarkdown(
  conversation: ExportableConversation,
  messages: readonly ExportableMessage[],
  options: ConversationExportOptions = {},
): string {
  const now = options.now ?? new Date();
  const title = oneLine(conversation.title ?? '') || UNTITLED_CONVERSATION;
  const spoken = messages.filter((message) => message.content.trim().length > 0);

  const meta = [
    options.spaceName ? oneLine(options.spaceName) : null,
    `${spoken.length} ${spoken.length === 1 ? 'message' : 'messages'}`,
    `updated ${stamp(conversation.updatedAt ?? conversation.createdAt, now)}`,
  ].filter(Boolean);

  const lines: string[] = [`# ${title}`, '', `_${meta.join(' · ')}_`, ''];

  for (const message of spoken) {
    lines.push(`## ${roleHeading(message.role)}`, '', message.content.trim(), '');
    if (message.role === 'user') continue;
    const { sources, verification, turn } = answerFacts(message, options);
    lines.push(...sourcesBlock(sources));
    const checked = verificationLine(verification);
    if (checked) lines.push(`_${checked}_`, '');
    const cost = turnLine(turn);
    if (cost) lines.push(`_${cost}_`, '');
  }

  return `${lines.join('\n').trimEnd()}\n`;
}

export interface AnswerExportInput {
  content: string;
  sources?: readonly SourceWithMetadata[];
  verification?: MessageVerificationSummary | null;
  /** Where the answer came from, so the journal page says so. */
  conversationTitle?: string | null;
  /** `metadata.turn`, read defensively — it may not exist yet. */
  turn?: unknown;
  capturedAt?: Date;
}

/**
 * One answer as a journal block, in the shape `buildSynthesisBlock` writes:
 * a heading that says where it came from, a stamp, the answer, its sources.
 */
export function answerToMarkdown(input: AnswerExportInput): string {
  const captured = input.capturedAt ?? new Date();
  const from = oneLine(input.conversationTitle ?? '') || UNTITLED_CONVERSATION;
  const lines = [
    `## From chat · ${from}`,
    `_Saved ${captured.toLocaleString()}._`,
    '',
    input.content.trim(),
    '',
    ...sourcesBlock(input.sources ?? []),
  ];
  const checked = verificationLine(input.verification);
  if (checked) lines.push(`_${checked}_`, '');
  const cost = turnLine(input.turn);
  if (cost) lines.push(`_${cost}_`, '');
  return `${lines.join('\n').trimEnd()}\n`;
}

/** The files a checked sentence rests on, as "file, location" phrases. */
function claimCitations(
  verdict: ClaimVerdict,
  citationMap: ReadonlyMap<number, SourceWithMetadata>,
): string[] {
  return verdict.citationIds.flatMap((id) => {
    const source = citationMap.get(id);
    if (!source) return [];
    const name = oneLine(sanitizeFileName(source.fileName || source.filePath || 'Untitled source'));
    const location = isWebSource(source) ? oneLine(source.filePath ?? '') : formatSourceLocation(source);
    return [location ? `${name}, ${location}` : name];
  });
}

/**
 * A sentence and where it came from, for pasting somewhere that has no idea
 * what this chat was. An uncited sentence says so rather than implying a
 * source it never had.
 */
export function claimWithCitation(
  verdict: ClaimVerdict,
  citationMap: ReadonlyMap<number, SourceWithMetadata>,
): string {
  const sentence = verdict.sentence.trim();
  const citations = claimCitations(verdict, citationMap);
  if (citations.length === 0) return sentence;
  return `${sentence}\n\n— ${citations.join('; ')}`;
}

/** How a verdict reads in prose, including which check produced it. */
export function claimVerdictLine(verdict: ClaimVerdict): string {
  const how =
    verdict.method === 'judge'
      ? 'the model read the passage against it'
      : 'matched on shared wording, not read for meaning';
  if (verdict.verdict === 'supported') return `Backed by the source — ${how}.`;
  if (verdict.verdict === 'contradicted') return `The source says otherwise — ${how}.`;
  return verdict.citationIds.length > 0
    ? `Not found in the cited passage — ${how}.`
    : 'No source was cited for this sentence.';
}

/** A checked sentence as a journal block: the quote, its sources, the verdict. */
export function claimToMarkdown(
  verdict: ClaimVerdict,
  citationMap: ReadonlyMap<number, SourceWithMetadata>,
  options: { conversationTitle?: string | null; capturedAt?: Date } = {},
): string {
  const captured = options.capturedAt ?? new Date();
  const from = oneLine(options.conversationTitle ?? '') || UNTITLED_CONVERSATION;
  const citations = claimCitations(verdict, citationMap);
  const lines = [
    `## From chat · ${from}`,
    `_Saved ${captured.toLocaleString()}._`,
    '',
    `> ${oneLine(verdict.sentence)}`,
    '',
  ];
  if (citations.length > 0) lines.push(`— ${citations.join('; ')}`, '');
  if (verdict.evidenceQuote?.trim()) {
    lines.push(`Evidence: “${oneLine(verdict.evidenceQuote)}”`, '');
  }
  lines.push(`_${claimVerdictLine(verdict)}_`, '');
  return `${lines.join('\n').trimEnd()}\n`;
}

/**
 * The composer prefill behind "Ask why".
 *
 * Every line is quoted by the receiving screen, so the question travels inside
 * the quote block rather than beside it. An ungrounded sentence asks for the
 * source instead of asking for the reasoning: there may not be any.
 */
export function askWhyPrefill(verdict: ClaimVerdict): string {
  const question =
    verdict.verdict === 'supported'
      ? 'Why does the cited passage support this? Quote the words it rests on.'
      : verdict.verdict === 'contradicted'
        ? 'The check says my sources contradict this. Which passage, and what does it actually say?'
        : 'Where in my documents does this come from? If it is not there, say so plainly.';
  return `${oneLine(verdict.sentence)}\n\n${question}`;
}
