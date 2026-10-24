/**
 * Single canonical source-of-truth adapter: turns every source or document
 * representation across Lattice into a `SourceWithMetadata` + optional
 * `PassageLocator` for `SourceReaderBody` / `FilePreviewModal` / `ChatReaderPane`.
 *
 * No surface constructs ad-hoc reader payloads or kicks users out to external
 * browsers or legacy viewers: every surface speaks through these constructors.
 */

import type {
  ConversationLinkedDocumentDto,
  LearningEvidencePassageDto,
  LearningSourceLibraryItemDto,
  LearningSourceVersionDto,
  RecentDocument,
  StudySourceDto,
} from '@/lib/bindings';
import { formatSourceLocation } from '@/shared/sources/passageLocator';
import type { DocumentMetadata, SearchResult } from '@/types';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';
import { mimeTypeForPath } from '@/utils/mimeTypes';
import { getSourceExternalUrl, isHttpUrl } from '@/utils/sourcePreview';

export interface ResolvedReaderTarget {
  source: SourceWithMetadata;
  locator: PassageLocator | null;
}

const MIME_BY_FILE_TYPE: Record<string, string> = {
  pdf: 'application/pdf',
  md: 'text/markdown',
  markdown: 'text/markdown',
  txt: 'text/plain',
  text: 'text/plain',
  html: 'text/html',
  htm: 'text/html',
  web: 'text/html',
  web_article: 'text/html',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  wav: 'audio/wav',
};

function resolveMimeType(fileType?: string | null, filePath?: string | null, category?: string | null): string {
  const raw = (fileType ?? '').trim().toLowerCase();
  if (raw.includes('/')) return raw;
  if (raw && MIME_BY_FILE_TYPE[raw]) return MIME_BY_FILE_TYPE[raw];
  const fromPath = filePath ? mimeTypeForPath(filePath) : '';
  if (fromPath) return fromPath;
  const cat = (category ?? '').trim().toLowerCase();
  if (cat.includes('web') || (filePath && isHttpUrl(filePath))) return 'text/html';
  return 'text/plain';
}

function fileNameFromPathOrUrl(pathOrUrl: string, fallback = 'Untitled source'): string {
  const trimmed = pathOrUrl.trim();
  if (!trimmed) return fallback;
  if (isHttpUrl(trimmed)) {
    try {
      const parsed = new URL(trimmed);
      const lastSegment = parsed.pathname.split('/').filter(Boolean).pop();
      return lastSegment ? decodeURIComponent(lastSegment) : parsed.hostname.replace(/^www\./, '');
    } catch {
      return trimmed;
    }
  }
  const parts = trimmed.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] ?? fallback;
}

/** Construct a reader target from a bare file path or URL (used by legacy callers). */
export function fromFilePath(filePath: string, title?: string): ResolvedReaderTarget {
  const trimmed = filePath.trim();
  const web = isHttpUrl(trimmed);
  const fileName = title?.trim() || fileNameFromPathOrUrl(trimmed);
  return {
    source: {
      documentId: web ? `web:${trimmed}` : trimmed,
      chunkId: web ? `web:${trimmed}` : `${trimmed}#0`,
      fileName,
      filePath: trimmed,
      mimeType: resolveMimeType(null, trimmed, web ? 'web' : 'document'),
      category: web ? 'web' : 'document',
      content: '',
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: new Date(0).toISOString(),
    },
    locator: null,
  };
}

/** Library (`FileBrowser`) document -> unified reader target. */
export function fromDocumentMetadata(doc: DocumentMetadata): ResolvedReaderTarget {
  const web =
    isHttpUrl(doc.filePath) ||
    doc.category.toLowerCase().includes('web') ||
    doc.fileType.toLowerCase().includes('web');
  return {
    source: {
      documentId: doc.id,
      chunkId: `${doc.id}#0`,
      fileName: doc.fileName || fileNameFromPathOrUrl(doc.filePath),
      filePath: doc.filePath,
      mimeType: resolveMimeType(doc.fileType, doc.filePath, doc.category),
      category: doc.category || (web ? 'web' : 'document'),
      content: '',
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: doc.modifiedAt || doc.indexedAt || new Date(0).toISOString(),
    },
    locator: null,
  };
}

/** Global Search (`SearchInterface`) result -> unified reader target with passage locator. */
export function fromSearchResult(result: SearchResult): ResolvedReaderTarget {
  const meta = result.metadata ?? {};
  const rawPath = (result.path ?? (typeof meta.path === 'string' ? meta.path : '') ?? '').trim();
  const url = typeof meta.url === 'string' && isHttpUrl(meta.url) ? meta.url.trim() : isHttpUrl(rawPath) ? rawPath : null;
  const effectivePath = url ?? rawPath;
  const documentId = (result.documentId ?? result.id ?? effectivePath).trim();
  const category = typeof meta.category === 'string' ? meta.category : url ? 'web' : 'document';
  const fileType = typeof meta.file_type === 'string' ? meta.file_type : null;
  const fileSize = typeof meta.file_size === 'number' && Number.isFinite(meta.file_size) ? meta.file_size : 0;
  const modifiedAt =
    (typeof meta.updated_at === 'string' && meta.updated_at) ||
    (typeof meta.created_at === 'string' && meta.created_at) ||
    new Date(0).toISOString();
  const pageNumber =
    typeof meta.page_number === 'number'
      ? meta.page_number
      : typeof meta.pageNumber === 'number'
        ? meta.pageNumber
        : undefined;
  const section = typeof meta.section === 'string' ? meta.section : undefined;
  const chunkId = result.id || `${documentId}#${result.position ?? 0}`;
  const snippet = (result.content ?? '').trim();

  const source: SourceWithMetadata = {
    documentId: documentId || 'search-result',
    chunkId,
    fileName: result.title?.trim() || (typeof meta.filename === 'string' ? meta.filename : '') || fileNameFromPathOrUrl(effectivePath),
    filePath: effectivePath || documentId,
    mimeType: resolveMimeType(fileType, effectivePath, category),
    category,
    content: snippet,
    excerpt: snippet || undefined,
    highlights: result.highlights,
    section,
    chunkIndex: typeof result.position === 'number' ? result.position : undefined,
    pageNumber,
    score: Number.isFinite(result.score) ? Math.max(0, result.score) : 1,
    fileSizeBytes: Math.max(0, fileSize),
    modifiedAt,
  };

  const locator: PassageLocator | null =
    snippet || pageNumber !== undefined || typeof result.position === 'number'
      ? {
          text: snippet,
          chunkId,
          chunkIndex: typeof result.position === 'number' ? result.position : undefined,
          page: pageNumber,
          highlights: result.highlights,
          label: formatSourceLocation({ section, chunkId, pageNumber }) ?? undefined,
        }
      : null;

  return { source, locator };
}

/** Dashboard (`RecentDocument`) -> unified reader target. */
export function fromRecentDocument(doc: RecentDocument): ResolvedReaderTarget {
  const id = doc.documentId || doc.id;
  const filePath = doc.documentPath || id;
  const web = isHttpUrl(filePath);
  return {
    source: {
      documentId: id,
      chunkId: `${id}#0`,
      fileName: doc.documentName || fileNameFromPathOrUrl(filePath),
      filePath,
      mimeType: resolveMimeType(doc.fileType, filePath, web ? 'web' : 'document'),
      category: web ? 'web' : 'document',
      content: '',
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: doc.lastAccessedAt || new Date(0).toISOString(),
    },
    locator: null,
  };
}

/** Flashcard / Study recall citation (`StudySourceDto`) -> unified reader target. */
export function fromStudySource(
  studySource: StudySourceDto,
  index = 0,
  conversationId?: string,
): ResolvedReaderTarget {
  const externalUrl = studySource.url?.trim() && isHttpUrl(studySource.url) ? studySource.url.trim() : null;
  const rawPath = studySource.filePath?.trim() || '';
  const effectivePath = externalUrl ?? rawPath;
  const web = Boolean(externalUrl || isHttpUrl(effectivePath));
  const excerpt = studySource.excerpt?.trim() || '';
  const documentId = studySource.documentId?.trim() || (web ? `web:${effectivePath}` : effectivePath || `study-${index}`);
  const chunkId = studySource.chunkId?.trim() || `${documentId}#${index}`;

  const source: SourceWithMetadata = {
    documentId,
    chunkId,
    fileName: studySource.fileName?.trim() || fileNameFromPathOrUrl(effectivePath, `Citation ${index + 1}`),
    filePath: effectivePath || documentId,
    mimeType: resolveMimeType(null, effectivePath, web ? 'web' : 'document'),
    category: web ? 'web' : 'document',
    content: excerpt,
    excerpt: excerpt || undefined,
    score: 1,
    fileSizeBytes: 0,
    modifiedAt: new Date(0).toISOString(),
    citationId: index + 1,
    conversationId,
  };

  const locator: PassageLocator | null = excerpt
    ? {
        text: excerpt,
        chunkId,
      }
    : null;

  return { source, locator };
}

/** Chat linked document (`ConversationLinkedDocumentDto`) -> unified reader target. */
export function fromConversationLinkedDocument(
  doc: ConversationLinkedDocumentDto,
  conversationId?: string,
): ResolvedReaderTarget {
  const web = isHttpUrl(doc.filePath) || doc.category.toLowerCase().includes('web');
  return {
    source: {
      documentId: doc.documentId,
      chunkId: `${doc.documentId}#0`,
      fileName: doc.fileName || fileNameFromPathOrUrl(doc.filePath),
      filePath: doc.filePath,
      mimeType: resolveMimeType(doc.fileType, doc.filePath, doc.category),
      category: doc.category || (web ? 'web' : 'document'),
      content: '',
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: doc.lastReferencedAt || doc.indexedAt || new Date(0).toISOString(),
      conversationId,
    },
    locator: null,
  };
}

export interface WebSourceInput {
  url: string;
  title?: string | null;
  label?: string | null;
  excerpt?: string | null;
  content?: string | null;
  fetchedAt?: string | null;
  truncated?: boolean;
}

/**
 * Chat web source or ResearchDeck page -> unified reader target.
 * If the conversation already holds a full `SourceWithMetadata` (with `webSnapshot`)
 * for the same URL, enriches from it so Reader view has the full captured text.
 */
export function fromConversationWebSource(
  web: WebSourceInput,
  conversationId?: string,
  existingSources?: readonly SourceWithMetadata[],
): ResolvedReaderTarget {
  const url = web.url.trim();
  const normalizedTarget = url.toLowerCase().replace(/\/+$/, '');
  const matched = existingSources?.find((candidate) => {
    const candidateUrl = getSourceExternalUrl(candidate) ?? (isHttpUrl(candidate.filePath) ? candidate.filePath : null);
    if (!candidateUrl) return false;
    return candidateUrl.trim().toLowerCase().replace(/\/+$/, '') === normalizedTarget;
  });

  if (matched) {
    return {
      source: {
        ...matched,
        conversationId: conversationId ?? matched.conversationId,
      },
      locator: null,
    };
  }

  const title = web.title?.trim() || web.label?.trim() || fileNameFromPathOrUrl(url, url);
  const fullText = web.content?.trim() || '';
  const excerpt = web.excerpt?.trim() || fullText;

  return {
    source: {
      documentId: `web:${url}`,
      chunkId: `web:${url}`,
      fileName: title,
      filePath: url,
      mimeType: 'text/html',
      category: 'web',
      content: excerpt,
      excerpt: excerpt || undefined,
      webSnapshot: fullText
        ? {
            url,
            title,
            text: fullText,
            fetchedAt: web.fetchedAt ?? null,
            truncated: Boolean(web.truncated),
          }
        : undefined,
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: web.fetchedAt ?? new Date(0).toISOString(),
      conversationId,
    },
    locator: null,
  };
}

export interface JournalSourceInput {
  documentId?: string | null;
  fileName: string;
  filePath: string;
  excerpt?: string;
  category?: string;
  citationIndex?: number;
}

/** Journal entry source (`EntryFromConversation`) -> unified reader target. */
export function fromJournalSource(
  item: JournalSourceInput,
  conversationId?: string,
  existingSources?: readonly SourceWithMetadata[],
): ResolvedReaderTarget {
  const rawPath = item.filePath.trim();
  const pathFromUri = rawPath.startsWith('file://')
    ? decodeURIComponent(rawPath.replace(/^file:\/\//, ''))
    : rawPath;
  const web = isHttpUrl(pathFromUri) || (item.category ?? '').toLowerCase().includes('web');

  if (web && existingSources?.length) {
    const normalizedTarget = pathFromUri.toLowerCase().replace(/\/+$/, '');
    const matched = existingSources.find((candidate) => {
      const candidateUrl = getSourceExternalUrl(candidate) ?? (isHttpUrl(candidate.filePath) ? candidate.filePath : null);
      return candidateUrl?.trim().toLowerCase().replace(/\/+$/, '') === normalizedTarget;
    });
    if (matched) {
      const excerpt = item.excerpt?.trim() || matched.excerpt || matched.content;
      return {
        source: {
          ...matched,
          excerpt: excerpt || matched.excerpt,
          citationId: item.citationIndex ?? matched.citationId,
          conversationId: conversationId ?? matched.conversationId,
        },
        locator: excerpt ? { text: excerpt, chunkId: matched.chunkId } : null,
      };
    }
  }

  const excerpt = item.excerpt?.trim() || '';
  const documentId = item.documentId?.trim() || (web ? `web:${pathFromUri}` : pathFromUri || item.fileName);
  const chunkId = `${documentId}#${item.citationIndex ?? 0}`;

  return {
    source: {
      documentId,
      chunkId,
      fileName: item.fileName?.trim() || fileNameFromPathOrUrl(pathFromUri),
      filePath: pathFromUri,
      mimeType: resolveMimeType(null, pathFromUri, item.category ?? (web ? 'web' : 'document')),
      category: item.category || (web ? 'web' : 'document'),
      content: excerpt,
      excerpt: excerpt || undefined,
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: new Date(0).toISOString(),
      citationId: item.citationIndex,
      conversationId,
    },
    locator: excerpt ? { text: excerpt, chunkId } : null,
  };
}

/** Learning Studio library item (`LearningSourceLibraryItemDto`) -> unified reader target. */
export function fromLearningSource(
  item: LearningSourceLibraryItemDto,
  detail?: LearningSourceVersionDto | null,
): ResolvedReaderTarget {
  const version = detail?.version ?? item.activeVersion ?? item.pendingVersion ?? item.versions[0] ?? null;
  const resolvedUrl = version?.resolvedUrl ?? item.requestedUrl ?? (isHttpUrl(item.origin) ? item.origin : null);
  const web = item.kind === 'web' || Boolean(resolvedUrl && isHttpUrl(resolvedUrl));
  const title =
    version?.title?.trim() ||
    item.requestedUrl?.trim() ||
    (item.kind === 'document' ? 'Library document' : 'Saved source');
  const fullText = detail?.fullText?.trim() || version?.excerpt?.trim() || '';
  const acquiredIso = version?.acquiredAt ? new Date(version.acquiredAt).toISOString() : new Date(item.updatedAt).toISOString();
  const filePath = resolvedUrl ?? item.origin;
  const documentId = item.kind === 'document' && item.origin ? item.origin : web && resolvedUrl ? `web:${resolvedUrl}` : item.id;

  return {
    source: {
      documentId,
      chunkId: version?.id ?? `${documentId}#0`,
      fileName: title,
      filePath,
      mimeType: web ? 'text/html' : resolveMimeType(null, filePath, item.kind),
      category: web ? 'web' : item.kind,
      content: fullText,
      excerpt: version?.excerpt?.trim() || undefined,
      webSnapshot:
        web && fullText
          ? {
              url: resolvedUrl ?? filePath,
              title,
              text: fullText,
              fetchedAt: acquiredIso,
              truncated: Boolean(version?.truncated),
            }
          : undefined,
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: acquiredIso,
    },
    locator: null,
  };
}

/** Learning Studio lesson evidence passage (`LearningEvidencePassageDto`) -> unified reader target. */
export function fromLearningEvidencePassage(
  passage: LearningEvidencePassageDto,
  index = 0,
  fullText?: string | null,
): ResolvedReaderTarget {
  const url = passage.url?.trim() && isHttpUrl(passage.url) ? passage.url.trim() : null;
  const web = Boolean(url);
  const text = passage.text?.trim() || '';
  const documentId = url ? `web:${url}` : passage.sourceVersionId || `passage-${index}`;
  const chunkId = `${passage.sourceVersionId}:${passage.startByte}-${passage.endByte}`;

  return {
    source: {
      documentId,
      chunkId,
      fileName: passage.title?.trim() || (url ? fileNameFromPathOrUrl(url) : `Source ${index + 1}`),
      filePath: url ?? passage.sourceVersionId,
      mimeType: web ? 'text/html' : 'text/plain',
      category: web ? 'web' : 'document',
      content: fullText?.trim() || text,
      excerpt: text || undefined,
      webSnapshot:
        web && (fullText?.trim() || text)
          ? {
              url: url!,
              title: passage.title?.trim() || null,
              text: fullText?.trim() || text,
              fetchedAt: null,
              truncated: false,
            }
          : undefined,
      score: 1,
      fileSizeBytes: 0,
      modifiedAt: new Date(0).toISOString(),
      citationId: index + 1,
    },
    locator: text
      ? {
          text,
          chunkId,
        }
      : null,
  };
}
