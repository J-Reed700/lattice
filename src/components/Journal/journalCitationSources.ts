import type { SourceDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';

/** Convert the nullable generated DTO fields to the chat reader's runtime shape. */
export function toJournalCitationSource(source: SourceDto): SourceWithMetadata {
  return {
    documentId: source.documentId,
    chunkId: source.chunkId,
    fileName: source.fileName,
    filePath: source.filePath,
    mimeType: source.mimeType,
    category: source.category,
    content: source.content,
    ...(source.webSnapshot != null ? { webSnapshot: source.webSnapshot } : {}),
    score: source.score,
    fileSizeBytes: source.fileSizeBytes,
    modifiedAt: source.modifiedAt,
    ...(source.citationId != null ? { citationId: source.citationId } : {}),
    ...(source.excerpt != null ? { excerpt: source.excerpt } : {}),
    ...(source.highlights != null ? { highlights: source.highlights } : {}),
    ...(source.section != null ? { section: source.section } : {}),
    ...(source.chunkIndex != null ? { chunkIndex: source.chunkIndex } : {}),
    ...(source.pageNumber != null ? { pageNumber: source.pageNumber } : {}),
    ...(source.chunkExcerpts != null
      ? {
          chunkExcerpts: source.chunkExcerpts.map((chunk) => ({
            chunkId: chunk.chunkId,
            excerpt: chunk.excerpt,
            score: chunk.score,
            ...(chunk.section != null ? { section: chunk.section } : {}),
            ...(chunk.chunkIndex != null ? { chunkIndex: chunk.chunkIndex } : {}),
            ...(chunk.pageNumber != null ? { pageNumber: chunk.pageNumber } : {}),
            ...(chunk.highlights != null ? { highlights: chunk.highlights } : {}),
          })),
        }
      : {}),
  };
}

export function journalCitationMap(
  sources: readonly SourceWithMetadata[],
): Map<number, SourceWithMetadata> {
  const citations = new Map<number, SourceWithMetadata>();
  for (const source of sources) {
    if (source.citationId && !citations.has(source.citationId)) {
      citations.set(source.citationId, source);
    }
  }
  return citations;
}
