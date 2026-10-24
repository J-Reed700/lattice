/**
 * Matches cited answer sentences to corresponding passage spans within web page text.
 */

export interface SourcedPassage {
  start: number;
  end: number;
  sentenceIndex: number;
  score: number;
}

const MAX_WINDOW = 3;
const MAX_SECTION_WINDOW = 10;
const SENTENCE_COST = 0.03;
const MIN_SCORE = 0.5;
const MIN_SHARED_TOKENS = 4;

const STOPWORDS = new Set([
  'a', 'about', 'above', 'after', 'again', 'against', 'all', 'also', 'am', 'an', 'and', 'any',
  'are', 'as', 'at', 'be', 'because', 'been', 'before', 'being', 'below', 'between', 'both',
  'but', 'by', 'can', 'cannot', 'could', 'did', 'do', 'does', 'doing', 'down', 'during', 'each',
  'few', 'for', 'from', 'further', 'had', 'has', 'have', 'having', 'he', 'her', 'here', 'hers',
  'herself', 'him', 'himself', 'his', 'how', 'however', 'i', 'if', 'in', 'into', 'is', 'it',
  'its', 'itself', 'just', 'me', 'more', 'most', 'much', 'must', 'my', 'myself', 'no', 'nor',
  'not', 'now', 'of', 'off', 'on', 'once', 'only', 'or', 'other', 'ought', 'our', 'ours',
  'ourselves', 'out', 'over', 'own', 'rather', 'same', 'she', 'should', 'so', 'some', 'such',
  'than', 'that', 'the', 'their', 'theirs', 'them', 'themselves', 'then', 'there', 'these',
  'they', 'this', 'those', 'through', 'to', 'too', 'under', 'until', 'up', 'very', 'was', 'we',
  'were', 'what', 'when', 'where', 'which', 'while', 'who', 'whom', 'why', 'will', 'with',
  'would', 'you', 'your', 'yours', 'yourself', 'yourselves',
]);

// Keeps decimals and thousands separators intact within tokens.
const TOKEN_PATTERN = /[\p{L}\p{N}]+(?:[.,][\p{N}]+)*/gu;

// Canonicalize word numbers; 'one' is omitted as it is predominantly a pronoun.
const NUMBER_WORDS = new Map<string, string>([
  ['two', '2'], ['three', '3'], ['four', '4'], ['five', '5'], ['six', '6'], ['seven', '7'],
  ['eight', '8'], ['nine', '9'], ['ten', '10'], ['eleven', '11'], ['twelve', '12'],
]);

function canonical(raw: string): string {
  const token = raw.toLowerCase();
  return NUMBER_WORDS.get(token) ?? token;
}

const CITATION_MARKER = /\[\d{1,4}(?:\s*,\s*\d{1,4})*\]/g;

interface WeightedToken {
  text: string;
  weight: number;
}

interface PageSentence {
  start: number;
  end: number;
  tokens: Set<string>;
  heading: boolean;
}

export interface SentenceSpan {
  start: number;
  end: number;
}

/** Extracts sentence spans from text by punctuation and line breaks. */
export function sentenceSpans(text: string): SentenceSpan[] {
  const spans: SentenceSpan[] = [];
  const boundary = /[.!?…]['"”’)\]]*(?=\s|$)|\n/g;

  const push = (from: number, to: number) => {
    let start = from;
    let end = to;
    while (start < end && /\s/.test(text[start]!)) start += 1;
    while (end > start && /\s/.test(text[end - 1]!)) end -= 1;
    if (end > start) spans.push({ start, end });
  };

  let cursor = 0;
  for (const match of text.matchAll(boundary)) {
    const end = (match.index ?? 0) + match[0].length;
    push(cursor, end);
    cursor = end;
  }
  push(cursor, text.length);
  return spans;
}

const MAX_HEADING_LENGTH = 80;
const SENTENCE_TAIL = /[.!?…,;:]$/;

function isTitled(line: string): boolean {
  return line.split(/\s+/).every((word) => {
    const bare = word.replace(/^[^\p{L}\p{N}]+/u, '').replace(/[^\p{L}\p{N}]+$/u, '');
    const first = bare[0];
    if (first === undefined || !/\p{L}/u.test(first)) return true;
    return STOPWORDS.has(bare.toLowerCase()) || first === first.toUpperCase();
  });
}

function sectionStarts(pageText: string): number[] {
  const starts: number[] = [];
  const lines = pageText.split('\n');
  let offset = 0;

  lines.forEach((line, index) => {
    const trimmed = line.trim();
    const alone =
      (index === 0 || !lines[index - 1]!.trim()) &&
      (index === lines.length - 1 || !lines[index + 1]!.trim());
    const titled =
      trimmed.length <= MAX_HEADING_LENGTH && !SENTENCE_TAIL.test(trimmed) && isTitled(trimmed);
    if (trimmed && alone && titled) {
      starts.push(offset + (line.length - line.trimStart().length));
    }
    offset += line.length + 1;
  });

  return starts;
}

export function stripCitationMarkers(text: string): string {
  return text.replace(CITATION_MARKER, ' ');
}

function isStopword(token: string): boolean {
  return STOPWORDS.has(token) || (token.length < 2 && !/\p{N}/u.test(token));
}

function weighTokens(sentence: string): WeightedToken[] {
  const stripped = stripCitationMarkers(sentence);
  const weights = new Map<string, number>();
  let position = 0;

  for (const match of stripped.matchAll(TOKEN_PATTERN)) {
    const raw = match[0];
    const token = canonical(raw);
    position += 1;
    if (isStopword(token)) continue;

    let weight = 1;
    if (/\p{N}/u.test(token)) {
      weight = 2.5;
    } else if (position > 1 && raw[0] !== raw[0]!.toLowerCase()) {
      // Mid-sentence capitalized words (names) are weighted higher than ordinary tokens.
      weight = 2;
    }
    weights.set(token, Math.max(weights.get(token) ?? 0, weight));
  }

  return Array.from(weights, ([text, weight]) => ({ text, weight }));
}

function pageTokens(text: string): Set<string> {
  const tokens = new Set<string>();
  for (const match of text.matchAll(TOKEN_PATTERN)) {
    const token = canonical(match[0]);
    if (!isStopword(token)) tokens.add(token);
  }
  return tokens;
}

function splitPage(pageText: string): PageSentence[] {
  const headings = new Set(sectionStarts(pageText));
  return sentenceSpans(pageText).map(({ start, end }) => ({
    start,
    end,
    tokens: pageTokens(pageText.slice(start, end)),
    heading: headings.has(start),
  }));
}

/** Merges overlapping or adjacent passage spans within the same section. */
export function mergePassages(
  passages: SourcedPassage[],
  pageText?: string
): SourcedPassage[] {
  const sorted = [...passages].sort((a, b) => a.start - b.start || a.end - b.end);
  const merged: SourcedPassage[] = [];
  const sections = pageText === undefined ? [] : sectionStarts(pageText);
  const touches = (end: number, start: number) =>
    start < end ||
    (pageText !== undefined && start >= end && !pageText.slice(end, start).trim());
  // Spans separated by a section heading are kept distinct.
  const parted = (end: number, start: number) => sections.some((at) => at >= end && at <= start);

  for (const passage of sorted) {
    const last = merged[merged.length - 1];
    if (last && touches(last.end, passage.start) && !parted(last.end, passage.start)) {
      last.end = Math.max(last.end, passage.end);
      if (passage.score > last.score) {
        last.score = passage.score;
        last.sentenceIndex = passage.sentenceIndex;
      }
      continue;
    }
    merged.push({ ...passage });
  }

  return merged;
}

interface Window {
  start: number;
  size: number;
  score: number;
  worth: number;
}

function bestWindow(
  page: readonly PageSentence[],
  hits: ReadonlyMap<number, ReadonlySet<string>>,
  weightOf: ReadonlyMap<string, number>,
  total: number,
  maxSize: number
): Window | null {
  let best: Window | null = null;
  const considered = new Set<string>();

  for (const hitIndex of Array.from(hits.keys()).sort((a, b) => a - b)) {
    for (let size = 1; size <= maxSize; size += 1) {
      for (let start = Math.max(0, hitIndex - size + 1); start <= hitIndex; start += 1) {
        if (start + size > page.length) continue;
        const key = `${start}:${size}`;
        if (considered.has(key)) continue;
        considered.add(key);

        // Windows cannot span across section headings.
        const matched = new Set<string>();
        let reachedPastHeading = false;
        for (let i = start; i < start + size; i += 1) {
          if (i > start && page[i]!.heading) {
            reachedPastHeading = true;
            break;
          }
          for (const token of hits.get(i) ?? []) matched.add(token);
        }
        if (reachedPastHeading || matched.size < MIN_SHARED_TOKENS) continue;

        let weight = 0;
        for (const token of matched) weight += weightOf.get(token) ?? 0;
        const score = weight / total;
        if (score < MIN_SCORE) continue;

        const worth = score - SENTENCE_COST * size;
        if (!best || worth > best.worth) best = { start, size, score, worth };
      }
    }
  }

  return best;
}

const CLAUSE_BREAK = /\s*;\s*|\s+[—–-]\s+|:\s+/;

function bestClauseWindow(
  sentence: string,
  locate: (_text: string, _maxSize: number) => Window | null
): Window | null {
  const clauses = stripCitationMarkers(sentence).split(CLAUSE_BREAK);
  if (clauses.length < 2) return null;
  let best: Window | null = null;
  for (const clause of clauses) {
    const window = locate(clause, MAX_WINDOW);
    if (window && (!best || window.worth > best.worth)) best = window;
  }
  return best;
}

/** Finds passage spans matching the given sentences within page text. */
export function findSourcedPassages(
  pageText: string,
  sentences: readonly string[]
): SourcedPassage[] {
  if (!pageText.trim() || sentences.length === 0) return [];

  const page = splitPage(pageText);
  if (page.length === 0) return [];

  const index = new Map<string, number[]>();
  page.forEach((sentence, sentenceIndex) => {
    for (const token of sentence.tokens) {
      const bucket = index.get(token);
      if (bucket) bucket.push(sentenceIndex);
      else index.set(token, [sentenceIndex]);
    }
  });

  const found: SourcedPassage[] = [];

  const locate = (text: string, maxSize: number): Window | null => {
    const tokens = weighTokens(text);
    if (tokens.length < MIN_SHARED_TOKENS) return null;
    const total = tokens.reduce((sum, token) => sum + token.weight, 0);
    if (total <= 0) return null;

    const weightOf = new Map(tokens.map((token) => [token.text, token.weight]));
    const hits = new Map<number, Set<string>>();
    for (const token of tokens) {
      for (const pageIndex of index.get(token.text) ?? []) {
        const bucket = hits.get(pageIndex);
        if (bucket) bucket.add(token.text);
        else hits.set(pageIndex, new Set([token.text]));
      }
    }
    if (hits.size === 0) return null;
    return bestWindow(page, hits, weightOf, total, maxSize);
  };

  sentences.forEach((sentence, sentenceIndex) => {
    // Try tight window first, then section window, then clause-level matching.
    const best =
      locate(sentence, MAX_WINDOW) ??
      locate(sentence, MAX_SECTION_WINDOW) ??
      bestClauseWindow(sentence, locate);
    if (!best) return;
    found.push({
      start: page[best.start]!.start,
      end: page[best.start + best.size - 1]!.end,
      sentenceIndex,
      score: best.score,
    });
  });

  return mergePassages(found, pageText);
}

const FOLDED = new Map<string, string>([
  ['‘', "'"], ['’', "'"], ['‚', "'"], ['‛', "'"],
  ['“', '"'], ['”', '"'], ['„', '"'],
  ['‐', '-'], ['‑', '-'], ['‒', '-'], ['–', '-'], ['—', '-'],
  [' ', ' '],
]);

function normalizeForQuote(text: string): { normalized: string; offsets: number[] } {
  const chars: string[] = [];
  const offsets: number[] = [];
  let pendingSpace = false;

  for (let i = 0; i < text.length; i += 1) {
    const raw = text[i]!;
    if (/\s/.test(raw)) {
      pendingSpace = chars.length > 0;
      continue;
    }
    if (pendingSpace) {
      chars.push(' ');
      offsets.push(i);
      pendingSpace = false;
    }
    const folded = FOLDED.get(raw) ?? raw;
    const lowered = folded.toLowerCase();
    // A character whose lowercase is longer than itself (ﬀ, İ) would shift
    // every offset after it, so it stays as it is.
    chars.push(lowered.length === 1 ? lowered : folded);
    offsets.push(i);
  }

  return { normalized: chars.join(''), offsets };
}

/** Finds the span of a quoted passage within page text, normalizing whitespace and punctuation. */
export function findQuoteSpan(pageText: string, quote: string): SentenceSpan | null {
  const needle = normalizeForQuote(quote).normalized.trim();
  if (needle.length < 12) return null;

  const haystack = normalizeForQuote(pageText);
  const at = haystack.normalized.indexOf(needle);
  if (at < 0) return null;

  const start = haystack.offsets[at];
  const end = haystack.offsets[at + needle.length - 1];
  if (start === undefined || end === undefined) return null;
  return { start, end: end + 1 };
}
