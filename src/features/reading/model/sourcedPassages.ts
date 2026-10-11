/**
 * Where on a web page the sentences that cite it appear.
 *
 * This is a wording match and nothing more: it finds the span of the page whose
 * words a cited sentence shares, which is what a reader checking a claim wants
 * to land on. It is not a record of what the model attended to, so everything
 * built on it has to say "matches" rather than "used".
 *
 * A weak match is worse than none — a highlight on the wrong paragraph tells
 * the reader the answer came from there — so the thresholds are deliberately
 * high and a sentence that clears neither of them produces nothing.
 */

/** A span of the page text one answer sentence matched. */
export interface SourcedPassage {
  /** Offsets into the page text, exact enough to slice with. */
  start: number;
  end: number;
  /** Which of the answer sentences matched here. */
  sentenceIndex: number;
  /** Weighted share of that sentence's content words found in the span, 0–1. */
  score: number;
}

/** At most this many consecutive page sentences count as one passage. */
const MAX_WINDOW = 3;
/**
 * What a window may grow to when no tight one matches: about one section of a
 * page, which is what an answer condenses into a single bullet.
 */
const MAX_SECTION_WINDOW = 10;
/**
 * What each sentence of a window costs it when windows are compared: a window
 * grows only for a sentence that brings more of the answer than this, so one
 * shared filler word cannot stretch a highlight into the next section.
 */
const SENTENCE_COST = 0.03;
/** Share of the answer sentence's weighted content words the span must carry. */
const MIN_SCORE = 0.5;
/** And that many distinct content words, so a short sentence cannot coast in. */
const MIN_SHARED_TOKENS = 4;

/**
 * Words that say nothing about what a page is about. Dropping them is what
 * stops "It is one of the most important parts of the system" from matching
 * every paragraph on the page.
 */
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

/**
 * Words and numbers. Decimals and thousands separators stay whole, because
 * "1.2 °C" splitting into "1" and "2" would match any page with a list on it.
 */
const TOKEN_PATTERN = /[\p{L}\p{N}]+(?:[.,][\p{N}]+)*/gu;

/**
 * Small numbers as a page spells them. An answer writes "6–8 hours" where the
 * page says "six to eight hours", and numbers are what a match leans on most.
 * No "one": it is a pronoun far more often than it is a count.
 */
const NUMBER_WORDS = new Map<string, string>([
  ['two', '2'], ['three', '3'], ['four', '4'], ['five', '5'], ['six', '6'], ['seven', '7'],
  ['eight', '8'], ['nine', '9'], ['ten', '10'], ['eleven', '11'], ['twelve', '12'],
]);

/** A word as both sides of a match spell it. */
function canonical(raw: string): string {
  const token = raw.toLowerCase();
  return NUMBER_WORDS.get(token) ?? token;
}

/** `[3]`, `[1][3]` and `[1, 3]` — every form the answers actually carry. */
const CITATION_MARKER = /\[\d{1,4}(?:\s*,\s*\d{1,4})*\]/g;

interface WeightedToken {
  text: string;
  weight: number;
}

interface PageSentence {
  start: number;
  end: number;
  tokens: Set<string>;
  /** True when this "sentence" is the heading a section opens with. */
  heading: boolean;
}

/** A sentence span, as offsets into the text it came from. */
export interface SentenceSpan {
  start: number;
  end: number;
}

/**
 * Sentence boundaries, shared by both sides of the match so that "a sentence"
 * means the same thing in an answer and on a page.
 *
 * A line break ends a sentence too: a page's extracted text is paragraphs and
 * headings, and a heading rarely ends in a full stop.
 */
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

/**
 * A heading is no longer than this. Past it, a line with no full stop is a
 * paragraph that ran out of punctuation rather than a title.
 */
const MAX_HEADING_LENGTH = 80;

/** A line ending like this is prose, however short it is. */
const SENTENCE_TAIL = /[.!?…,;:]$/;

/**
 * Whether a line is titled: every word carrying meaning starts with a capital.
 *
 * Without this, any short line that forgot its full stop would end a section —
 * "Seed to harvest: 24 to 30 days" is the last line of one, not the first line
 * of the next. Function words are left alone because a title lowercases them
 * on purpose ("Broccoli For Strong Spring Crops" either way).
 */
function isTitled(line: string): boolean {
  return line.split(/\s+/).every((word) => {
    const bare = word.replace(/^[^\p{L}\p{N}]+/u, '').replace(/[^\p{L}\p{N}]+$/u, '');
    const first = bare[0];
    if (first === undefined || !/\p{L}/u.test(first)) return true;
    return STOPWORDS.has(bare.toLowerCase()) || first === first.toUpperCase();
  });
}

/**
 * Where each section of the page begins.
 *
 * Extracted text keeps almost nothing of a page's structure, but it keeps
 * this: a short titled line standing alone between blank lines and ending in
 * no punctuation — "Broccoli For Strong Spring Crops", "Bok Choy". It is the
 * page saying it has changed subject, and both halves of this module take it
 * at its word: a window never reaches past a heading, and two spans either
 * side of one stay two spans.
 */
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

/** The text with `[n]` markers taken out, so they cannot count as content. */
export function stripCitationMarkers(text: string): string {
  return text.replace(CITATION_MARKER, ' ');
}

function isStopword(token: string): boolean {
  return STOPWORDS.has(token) || (token.length < 2 && !/\p{N}/u.test(token));
}

/**
 * The content words of an answer sentence, with what each is worth.
 *
 * Numbers and names carry the weight because they are what a reader checks: a
 * paragraph that shares "1.2" and "Halvorsen" with the answer is the paragraph
 * the answer is about, however many ordinary words it happens to share.
 */
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
      // Sentence-initial capitals say nothing; a capital mid-sentence is a name.
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

/**
 * Overlapping spans folded together, in page order.
 *
 * Two answer sentences drawn from the same paragraph are one highlight: the
 * reader is being shown a place on the page, not a count of sentences. Given
 * the page text, spans with nothing but whitespace between them count as
 * overlapping — two marks a space apart are one mark to the eye anyway.
 */
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
  // Two spans with a heading between them are in two sections of the page,
  // however little text separates them — the blank line before a heading is
  // the same blank line as any other.
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
  /** `score` less what the window's length costs it; what windows compete on. */
  worth: number;
}

/**
 * The run of at most `maxSize` consecutive page sentences that carries the most
 * of an answer sentence for its length, if any run carries enough.
 */
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

        // A heading opens a section, so a window that reached past one would
        // be pointing at two of them, and the answer sentence was written off
        // at most one. A window may still begin at a heading: a title and what
        // it introduces are one place on the page.
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

/** Where an answer sentence changes subject: `;`, a spaced dash, a colon. */
const CLAUSE_BREAK = /\s*;\s*|\s+[—–-]\s+|:\s+/;

/**
 * The best tight window any one clause of `sentence` has, each clause held to
 * the whole bar on its own. Never the section window: a clause is a few words,
 * and a few words scattered over a section are not a passage.
 */
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

/**
 * The spans of `pageText` that the given answer sentences match.
 *
 * Each sentence claims at most one span — its best window of one to three
 * consecutive page sentences, or failing that of about a section — and only if
 * that window carries enough of the sentence's content to be worth pointing at.
 */
export function findSourcedPassages(
  pageText: string,
  sentences: readonly string[]
): SourcedPassage[] {
  if (!pageText.trim() || sentences.length === 0) return [];

  const page = splitPage(pageText);
  if (page.length === 0) return [];

  // Token to the page sentences holding it, so a sentence only ever looks at
  // the parts of the page that share a word with it.
  const index = new Map<string, number[]>();
  page.forEach((sentence, sentenceIndex) => {
    for (const token of sentence.tokens) {
      const bucket = index.get(token);
      if (bucket) bucket.push(sentenceIndex);
      else index.set(token, [sentenceIndex]);
    }
  });

  const found: SourcedPassage[] = [];

  // The best window for `text` of at most `maxSize` page sentences.
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
    // A tight window first. An answer that boils a whole section down to one
    // line shares too little with any three sentences of it, so only then is
    // the section-sized window tried — against the same bar, never a lower one.
    // Last, the sentence a clause at a time: one that credits two sources takes
    // a clause from each, and neither page holds half of the whole.
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

/** The typographic characters a page and a quote taken from it disagree about. */
const FOLDED = new Map<string, string>([
  ['‘', "'"], ['’', "'"], ['‚', "'"], ['‛', "'"],
  ['“', '"'], ['”', '"'], ['„', '"'],
  ['‐', '-'], ['‑', '-'], ['‒', '-'], ['–', '-'], ['—', '-'],
  [' ', ' '],
]);

/** Lowercase, straighten and collapse whitespace, one character at a time. */
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

/**
 * Where a quoted passage sits in the page text, if it is there at all.
 *
 * Only whitespace, case and the typographic characters a page and a quote
 * disagree about are forgiven. A quote that has been reworded is not found,
 * which is the point: it would otherwise be reported as an exact hit.
 */
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
