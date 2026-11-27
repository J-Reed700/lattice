import {
  SourceWithMetadataSchema,
  SourcesArraySchema,
} from '@/types/conversation';
import type { SourceWithMetadata } from '@/types/conversation';

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
    fromOcr: value.fromOcr ?? value.from_ocr ?? undefined,
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
