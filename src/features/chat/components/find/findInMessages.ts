import type { ConversationMessage } from '@/types/conversation';

/** The cap the Explorer search uses. */
export const MAX_FIND_RESULTS = 200;
const SNIPPET_BEFORE = 40;
const SNIPPET_AFTER = 100;

export interface MessageMatch {
  message: ConversationMessage;
  /** Occurrences in the message's markdown source. */
  count: number;
  /** The first occurrence with some text either side. */
  snippet: { before: string; match: string; after: string };
}

/**
 * Case-insensitive, and blind to how whitespace is spelled, so a query typed on
 * one line finds text that wraps in the source. Matches index the original
 * string (no lower-casing), so the snippet never drifts.
 */
export function findPattern(query: string): RegExp | null {
  const words = query.trim().split(/\s+/).filter(Boolean);
  if (!words.length) return null;
  return new RegExp(words.map(word => word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('\\s+'), 'giu');
}

/** Every user and assistant message containing the query, in conversation order. */
export function findInMessages(messages: readonly ConversationMessage[], query: string): { matches: MessageMatch[]; truncated: boolean } {
  const pattern = findPattern(query);
  const matches: MessageMatch[] = [];
  if (!pattern) return { matches, truncated: false };
  for (const message of messages) {
    if (message.role !== 'user' && message.role !== 'assistant') continue;
    const text = message.content.replace(/\s+/g, ' ');
    const found = Array.from(text.matchAll(pattern));
    if (!found.length) continue;
    if (matches.length === MAX_FIND_RESULTS) return { matches, truncated: true };
    const first = found[0];
    const start = first.index ?? 0;
    const end = start + first[0].length;
    matches.push({
      message,
      count: found.length,
      snippet: {
        before: (start > SNIPPET_BEFORE ? '…' : '') + text.slice(Math.max(0, start - SNIPPET_BEFORE), start).trimStart(),
        match: first[0],
        after: text.slice(end, end + SNIPPET_AFTER) + (end + SNIPPET_AFTER < text.length ? '…' : ''),
      },
    });
  }
  return { matches, truncated: false };
}

/** Occurrences counted in a message's rendered text, for the content they were counted on. */
export interface RenderedCount {
  content: string;
  count: number;
}

export interface FindRow extends MessageMatch {
  /** The conversation-wide index of the row's first occurrence. */
  start: number;
}

/**
 * Lay the matching messages out as one run of occurrences, the order the
 * counter and Enter walk them in. The source count stands in until a message
 * has rendered: then the count taken from what is on screen wins, because
 * markdown the reader never sees (a link's URL, a fence's language) can match
 * too. A message whose rendered text shows no match drops out.
 */
export function occurrenceRows(matches: readonly MessageMatch[], rendered: ReadonlyMap<string, RenderedCount>): { rows: FindRow[]; total: number } {
  const rows: FindRow[] = [];
  let total = 0;
  for (const match of matches) {
    const seen = rendered.get(match.message.id);
    const count = seen?.content === match.message.content ? seen.count : match.count;
    if (!count) continue;
    rows.push({ ...match, count, start: total });
    total += count;
  }
  return { rows, total };
}

/** The row holding occurrence `index`, and which of its occurrences that is. */
export function occurrenceAt(rows: readonly FindRow[], index: number): { row: FindRow; ordinal: number } | null {
  const row = rows.find(candidate => index >= candidate.start && index < candidate.start + candidate.count);
  return row ? { row, ordinal: index - row.start } : null;
}
