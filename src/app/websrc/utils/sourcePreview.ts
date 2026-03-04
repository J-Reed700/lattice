export type SourcePreviewKind = 'local-file' | 'archived-web' | 'external-web';

const WEB_ARCHIVE_SEGMENT = '/.recall/web-archive/';
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
