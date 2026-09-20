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
    const token = raw.toLowerCase();
    position += 1;
    if (isStopword(token)) continue;

    let weight = 1;
    if (/\p{N}/u.test(raw)) {
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
    const token = match[0].toLowerCase();
    if (!isStopword(token)) tokens.add(token);
  }
  return tokens;
}

function splitPage(pageText: string): PageSentence[] {
  return sentenceSpans(pageText).map(({ start, end }) => ({
    start,
    end,
    tokens: pageTokens(pageText.slice(start, end)),
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
  const touches = (end: number, start: number) =>
    start < end ||
    (pageText !== undefined && start >= end && !pageText.slice(end, start).trim());

  for (const passage of sorted) {
    const last = merged[merged.length - 1];
    if (last && touches(last.end, passage.start)) {
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

/**
 * The spans of `pageText` that the given answer sentences match.
 *
 * Each sentence claims at most one span — its best window of one to three
 * consecutive page sentences — and only if that window carries enough of the
 * sentence's content to be worth pointing at.
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

  sentences.forEach((sentence, sentenceIndex) => {
    const tokens = weighTokens(sentence);
    if (tokens.length < MIN_SHARED_TOKENS) return;
    const total = tokens.reduce((sum, token) => sum + token.weight, 0);
    if (total <= 0) return;

    const weightOf = new Map(tokens.map((token) => [token.text, token.weight]));
    const hits = new Map<number, Set<string>>();
    for (const token of tokens) {
      for (const pageIndex of index.get(token.text) ?? []) {
        const bucket = hits.get(pageIndex);
        if (bucket) bucket.add(token.text);
        else hits.set(pageIndex, new Set([token.text]));
      }
    }
    if (hits.size === 0) return;

    let best: { start: number; size: number; score: number } | null = null;
    const considered = new Set<string>();

    for (const hitIndex of Array.from(hits.keys()).sort((a, b) => a - b)) {
      for (let size = 1; size <= MAX_WINDOW; size += 1) {
        for (let start = Math.max(0, hitIndex - size + 1); start <= hitIndex; start += 1) {
          if (start + size > page.length) continue;
          const key = `${start}:${size}`;
          if (considered.has(key)) continue;
          considered.add(key);

          const matched = new Set<string>();
          for (let i = start; i < start + size; i += 1) {
            for (const token of hits.get(i) ?? []) matched.add(token);
          }
          if (matched.size < MIN_SHARED_TOKENS) continue;

          let weight = 0;
          for (const token of matched) weight += weightOf.get(token) ?? 0;
          const score = weight / total;
          if (score < MIN_SCORE) continue;

          if (!best || score > best.score || (score === best.score && size < best.size)) {
            best = { start, size, score };
          }
        }
      }
    }

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
