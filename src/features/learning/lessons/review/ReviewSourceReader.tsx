import { useLearningSourceVersion } from '@/features/learning/sources/useLearningSources';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { fromLearningEvidencePassage } from '@/features/reading/model/sourceAdapter';
import type { LearningEvidencePassageDto, SourceRefDto } from '@/lib/bindings';

/** What the reader opens: a source, and the sentence to scroll to and mark. */
export type ReaderRequest = { source: SourceRefDto; passage: LearningEvidencePassageDto | null; quote: string | null };

/**
 * "Open the source" : the whole source in the reader, read from the
 * same version detail the Sources tab uses, scrolled to the supporting
 * sentence, which the reader marks.
 */
export function ReviewSourceReader({ programId, request, onClose }: { programId: string; request: ReaderRequest | null; onClose: () => void }) {
  const source = request?.source ?? null;
  const detail = useLearningSourceVersion(source ? { programId, sourceId: source.sourceId, versionId: source.sourceVersionId } : null);
  const passage: LearningEvidencePassageDto | null = request
    ? request.passage ?? { sourceVersionId: request.source.sourceVersionId, title: request.source.title, url: request.source.url, text: '', startByte: 0, endByte: 0, retrievalKind: 'source', trust: null }
    : null;
  const target = passage ? fromLearningEvidencePassage(passage, 0, detail.data?.fullText ?? null) : null;
  const locator = target && request?.quote ? { ...(target.locator ?? { chunkId: target.source.chunkId }), text: request.quote } : target?.locator ?? null;
  return <FilePreviewModal isOpen={request !== null} onClose={onClose} source={target?.source ?? null} initialLocator={locator} />;
}
