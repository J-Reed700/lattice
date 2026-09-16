import { createElement, type ReactNode } from 'react';

export type SourcePreviewKind = 'local-file' | 'archived-web' | 'external-web';

const WEB_ARCHIVE_SEGMENT = '/.lattice/web-archive/';
const WEB_ARCHIVE_MARKDOWN_NAMES = new Set(['article.md', 'weblink.md']);
const WEB_ARTICLE_CATEGORY = 'web article';

export function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

function looksLikeHostPath(value: string): boolean {
  const trimmed = value.trim();
  return /^[a-z0-9.-]+\.[a-z]{2,}(?:[/:?#].*)?$/i.test(trimmed);
}

function isLikelyAbsoluteLocalPath(value: string): boolean {
  const trimmed = value.trim();
  return (
    trimmed.startsWith('/') ||
    /^[A-Za-z]:[\\/]/.test(trimmed) ||
    trimmed.startsWith('\\\\')
  );
}

function normalizeExternalUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (isHttpUrl(trimmed)) return trimmed;
  if (looksLikeHostPath(trimmed)) return `https://${trimmed}`;
  return null;
}

function safeDecodeUriComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function unwrapRedirectUrl(value: string): string {
  const normalized = normalizeExternalUrl(value);
  if (!normalized) return value;

  let parsed: URL;
  try {
    parsed = new URL(normalized);
  } catch {
    return normalized;
  }

  const wrapperHost = parsed.hostname.toLowerCase();
  const candidates: string[] = [];

  for (const [, rawParamValue] of parsed.searchParams.entries()) {
    if (!rawParamValue) continue;
    candidates.push(rawParamValue);
    const decoded = safeDecodeUriComponent(rawParamValue);
    if (decoded !== rawParamValue) {
      candidates.push(decoded);
      const decodedTwice = safeDecodeUriComponent(decoded);
      if (decodedTwice !== decoded) {
        candidates.push(decodedTwice);
      }
    }
  }

  for (const candidate of candidates) {
    const nested = normalizeExternalUrl(candidate);
    if (!nested) continue;

    try {
      const nestedParsed = new URL(nested);
      const nestedHost = nestedParsed.hostname.toLowerCase();
      if (nestedHost !== wrapperHost) {
        return nested;
      }
    } catch {
      // Ignore malformed nested candidate and continue scanning.
    }
  }

  return normalized;
}

export function isWebArchiveArticlePath(filePath: string): boolean {
  const normalizedPath = filePath.trim().toLowerCase();
  if (!normalizedPath.includes(WEB_ARCHIVE_SEGMENT)) {
    return false;
  }
  const fileName = normalizedPath.split('/').pop() || '';
  return WEB_ARCHIVE_MARKDOWN_NAMES.has(fileName);
}

export function getWebArchiveHtmlPath(filePath: string): string | null {
  if (!isWebArchiveArticlePath(filePath)) {
    return null;
  }
  const slashIndex = filePath.lastIndexOf('/');
  if (slashIndex === -1) {
    return null;
  }
  return `${filePath.slice(0, slashIndex)}/page.html`;
}

interface SourcePathLike {
  filePath?: string | null;
  path?: string | null;
  documentId?: string | null;
  category?: string | null;
  mimeType?: string | null;
}

export function getSourceExternalUrl(source: SourcePathLike): string | null {
  const filePath = source.filePath ?? '';
  const maybePath = source.path ?? '';
  const documentId = source.documentId ?? '';
  const webDocumentUrl = documentId.startsWith('web:')
    ? safeDecodeUriComponent(documentId.slice(4))
    : '';

  const raw =
    normalizeExternalUrl(filePath) ??
    normalizeExternalUrl(maybePath) ??
    normalizeExternalUrl(webDocumentUrl);
  if (!raw) return null;
  return unwrapRedirectUrl(raw);
}

export function getSourcePreviewKind(source: SourcePathLike): SourcePreviewKind {
  const filePath = source.filePath ?? '';
  const category = source.category?.toLowerCase().replace(/[_-]+/g, ' ').trim() ?? '';
  const mimeType = source.mimeType?.toLowerCase() ?? '';

  if (isWebArchiveArticlePath(filePath)) {
    return 'archived-web';
  }

  if (getSourceExternalUrl(source)) {
    return 'external-web';
  }

  if (
    (category.includes(WEB_ARTICLE_CATEGORY) || mimeType === 'text/html') &&
    !isLikelyAbsoluteLocalPath(filePath)
  ) {
    return 'external-web';
  }

  return 'local-file';
}


const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const termEntropy = (term: string) => {
  if (!term) return 0;
  const counts = new Map<string, number>();
  for (const ch of term) {
    counts.set(ch, (counts.get(ch) || 0) + 1);
  }
  let entropy = 0;
  for (const count of counts.values()) {
    const p = count / term.length;
    entropy -= p * Math.log2(p);
  }
  return entropy;
};

/**
 * How much a term is worth highlighting: character entropy scaled by length.
 * Marking every stop word would make the excerpt harder to read, not easier.
 */
export const termSalience = (term: string) => {
  const entropy = termEntropy(term);
  const lengthFactor = Math.log(term.length + 1);
  return entropy * (0.65 + 0.35 * lengthFactor);
};

/** The most informative of a set of search terms, at most `maxTerms`. */
export const selectInformativeTerms = (terms: string[], maxTerms: number) => {
  const normalized = Array.from(
    new Set(
      terms
        .map((term) => term.trim().toLowerCase())
        .filter((term) => term.length >= 3 && /[a-z]/i.test(term))
    )
  );
  if (normalized.length === 0) return [];

  const ranked = normalized
    .map((term) => ({ term, score: termSalience(term) }))
    .sort((a, b) => b.score - a.score || a.term.localeCompare(b.term));

  const bestScore = ranked[0]?.score ?? 0;
  if (bestScore <= Number.EPSILON) {
    return ranked.slice(0, maxTerms).map((entry) => entry.term);
  }

  const selected = ranked
    .filter((entry) => entry.score >= bestScore * 0.45)
    .slice(0, maxTerms)
    .map((entry) => entry.term);

  if (selected.length > 0) return selected;
  return ranked.slice(0, maxTerms).map((entry) => entry.term);
};

/** Excerpt text with the informative search terms wrapped in `<mark>`. */
export function renderHighlightedText(
  text: string,
  highlights?: string[]
): ReactNode {
  if (!highlights || highlights.length === 0) return text;

  const selected = selectInformativeTerms(highlights, 10);
  if (selected.length === 0) return text;

  const ordered = [...selected].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`\\b(${ordered.map(escapeRegExp).join('|')})\\b`, 'gi');
  const parts = text.split(pattern);
  const lookup = new Set(selected.map((term) => term.toLowerCase()));

  return parts.map((part, idx) =>
    lookup.has(part.toLowerCase())
      ? createElement(
          'mark',
          {
            key: `hl-${idx}`,
            className:
              'rounded-sm bg-[hsl(var(--accent-muted))] px-0.5 text-[hsl(var(--text-primary))]',
          },
          part
        )
      : createElement('span', { key: `hl-${idx}` }, part)
  );
}
