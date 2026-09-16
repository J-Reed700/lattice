import type { SourceWithMetadata } from '@/types/conversation';

/**
 * Citation Parsing Utilities
 *
 * Purpose: Parse answer text to identify and extract citation markers
 *
 * Supports formats:
 * - [1], [2], etc. (square brackets)
 * - (1), (2), etc. (parentheses)
 * - ¹, ², ³, etc. (superscript Unicode)
 */

export interface ParsedCitation {
  text: string;
  citationNumber?: number;
}

const MAX_TEXT_LENGTH = 100_000; // 100KB limit
const CITATION_TIMEOUT_MS = 100; // 100ms timeout
const MAX_CITATION_NUMBER = 99_999;

/**
 * Parse answer text into segments with citation markers.
 *
 * Splits text into alternating segments of plain text and citation markers.
 * Citation markers are identified by their number, plain text has no number.
 *
 * @param text - The text to parse (typically an assistant's answer)
 * @returns Array of segments with optional citation numbers
 *
 * @example
 * parseCitations("Paris[1] is the capital of France[2].")
 * // Returns:
 * // [
 * //   { text: "Paris", citationNumber: undefined },
 * //   { text: "", citationNumber: 1 },
 * //   { text: " is the capital of France", citationNumber: undefined },
 * //   { text: "", citationNumber: 2 },
 * //   { text: ".", citationNumber: undefined }
 * // ]
 */
export function parseCitations(text: string, sourceCount?: number): ParsedCitation[] {
  if (text.length > MAX_TEXT_LENGTH) {
    console.warn('[Citations] Text too long for parsing:', text.length, 'chars (max:', MAX_TEXT_LENGTH, ')');
    return [{
      text: `${text.substring(0, MAX_TEXT_LENGTH)  }... (text truncated)`,
      citationNumber: undefined,
    }];
  }

  const totalSources = Number.isInteger(sourceCount) && sourceCount && sourceCount > 0
    ? sourceCount
    : 0;
  let nextPlaceholderCitation = 1;
  const normalizedText = normalizeCitationArtifacts(text);

  const segments: ParsedCitation[] = [];

  // Use bounded quantifiers to prevent ReDoS
  // OLD: /\[(\d+)\]|\((\d+)\)|[¹²³⁴⁵⁶⁷⁸⁹⁰]+/g ← VULNERABLE
  const citationRegex = /\[(\d{1,5})\]|\((\d{1,5})\)|\[\^(\d{1,5})\]|\[#\]|\(#\)|[¹²³⁴⁵⁶⁷⁸⁹⁰]{1,5}/g;

  let lastIndex = 0;
  let match: RegExpExecArray | null;

  const startTime = Date.now();

  while ((match = citationRegex.exec(normalizedText)) !== null) {
    // Timeout protection (prevent infinite loop)
    if (Date.now() - startTime > CITATION_TIMEOUT_MS) {
      console.warn('[Citations] Parsing timeout after', CITATION_TIMEOUT_MS, 'ms');
      if (lastIndex < normalizedText.length) {
        segments.push({
          text: normalizedText.substring(lastIndex),
          citationNumber: undefined,
        });
      }
      break;
    }

    // Add text before citation
    if (match.index > lastIndex) {
      segments.push({
        text: normalizedText.slice(lastIndex, match.index),
        citationNumber: undefined,
      });
    }

    let number: number | undefined;
    const isPlaceholder = match[0] === '[#]' || match[0] === '(#)';

    if (isPlaceholder) {
      if (totalSources > 0) {
        number = nextPlaceholderCitation;
        nextPlaceholderCitation = nextPlaceholderCitation >= totalSources
          ? 1
          : nextPlaceholderCitation + 1;
      }
    } else {
      const numberStr = match[1] || match[2] || match[3] || superscriptToNumber(match[0]);
      const parsedNumber = parseInt(numberStr, 10);
      if (!isNaN(parsedNumber)) {
        number = parsedNumber;
      }
    }

    if (
      number !== undefined
      && number > 0
      && number <= MAX_CITATION_NUMBER
      && (totalSources === 0 || number <= totalSources)
    ) {
      segments.push({
        text: '',
        citationNumber: number,
      });
    } else {
      // Invalid number, treat as text
      if (match[0].trim().length > 0) {
        segments.push({
          text: match[0],
          citationNumber: undefined,
        });
      }
    }

    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < normalizedText.length) {
    segments.push({
      text: normalizedText.slice(lastIndex),
      citationNumber: undefined,
    });
  }

  // If no citations found, return text as single segment
  if (segments.length === 0) {
    segments.push({
      text: normalizedText,
      citationNumber: undefined,
    });
  }

  return segments;
}

/**
 * Convert superscript Unicode characters to regular numbers.
 *
 * @param superscript - String of superscript characters (e.g., "¹²³")
 * @returns Regular number string (e.g., "123")
 *
 * @example
 * superscriptToNumber("¹²³") // "123"
 */
function superscriptToNumber(superscript: string): string {
  const map: Record<string, string> = {
    '⁰': '0',
    '¹': '1',
    '²': '2',
    '³': '3',
    '⁴': '4',
    '⁵': '5',
    '⁶': '6',
    '⁷': '7',
    '⁸': '8',
    '⁹': '9',
  };

  return superscript
    .split('')
    .map((char) => map[char] || '0')
    .join('');
}

/**
 * Normalize common malformed citation variants into parseable forms.
 *
 * Handles:
 * - Markdown footnote references like [^1] -> [1]
 * - Placeholder footnotes like [^#] -> [#]
 * - Stray, malformed footnote blocks produced by some models
 */
function normalizeCitationArtifacts(text: string): string {
  // Normalize strict numeric/placeholder footnote references first.
  let normalized = text
    .replace(/\[\^(\d{1,5})\]/g, '[$1]')
    .replace(/\[\^#\]/g, '[#]');

  normalized = normalized.replace(/\[\^[^\]]+\](?:\{[^}]*\})*/g, '');

  return normalized;
}

/**
 * Map sources to citation numbers (1-indexed).
 *
 * Creates a mapping from citation number to source metadata.
 * This allows quick lookup of source details when rendering citations.
 *
 * @param sources - Array of source metadata from search results
 * @returns Map of citation number (1-indexed) to source metadata
 *
 * @example
 * const sources = [
 *   { file_name: "doc1.pdf", ... },
 *   { file_name: "doc2.pdf", ... }
 * ];
 * const map = createCitationMap(sources);
 * map.get(1) // { file_name: "doc1.pdf", ... }
 * map.get(2) // { file_name: "doc2.pdf", ... }
 */
export function createCitationMap(
  sources: SourceWithMetadata[]
): Map<number, SourceWithMetadata> {
  const map = new Map<number, SourceWithMetadata>();
  sources.forEach((source, index) => {
    // Prefer the id the backend assigned when building the prompt. Array
    // position is only a fallback for responses from an older backend that
    // didn't send one — relying on position is what made footnotes open the
    // wrong document whenever a document contributed more than one chunk, or
    // token budgeting trimmed the chunk list.
    const key = source.citationId ?? index + 1;
    if (!map.has(key)) {
      map.set(key, source);
    }
  });
  return map;
}
