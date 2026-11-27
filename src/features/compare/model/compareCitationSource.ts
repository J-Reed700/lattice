import type { CompareCellDto, CompareRowDto } from '@/types/api/compare';
import type { SourceWithMetadata } from '@/types/conversation';
import { mimeTypeForPath } from '@/utils/mimeTypes';

/** Map a cited compare cell to the shared citation card and reader metadata shape. */
export function compareCitationSource(
  row: CompareRowDto,
  cell: CompareCellDto,
): SourceWithMetadata | null {
  const citation = cell.citation;
  if (!citation) return null;
  return {
    documentId: row.documentId,
    chunkId: citation.chunkId,
    fileName: row.title,
    filePath: row.filePath,
    mimeType: mimeTypeForPath(row.filePath),
    category: '',
    content: citation.excerpt,
    excerpt: citation.excerpt,
    highlights: [],
    score: 1,
    fileSizeBytes: 0,
    // No mtime is known here; the comparison's own timestamp is not one.
    modifiedAt: '',
    citationId: citation.number,
    anchoredBy: [
      {
        occurrence: 0,
        relation: citation.support.relation,
        anchors: citation.support.anchors,
      },
    ],
    anchorBase: 0,
  };
}
