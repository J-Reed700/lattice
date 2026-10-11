/**
 * Corpus API types.
 *
 * Shapes for the vault-wide readouts: type mix, growth, document neighbourhoods
 * and the automatic themes produced by `corpus_shape`.
 */

export type CorpusTypeCountDto = import('../../lib/bindings').CorpusTypeCountDto;

export type CorpusShapeDto = import('../../lib/bindings').CorpusShapeDto;

export type CitingConversationDto = import('../../lib/bindings').CitingConversationDto;

export type SimilarDocumentDto = import('../../lib/bindings').SimilarDocumentDto;

export type ClusterLabelSource = 'llm' | 'inherited_exact' | 'inherited_jaccard' | 'fallback';

/**
 * One automatic theme. Called a "cluster" on the backend (the table and the
 * commands predate the name); the UI says "theme" everywhere.
 */
export type ClusterDto = import('../../lib/bindings').ClusterDto;

export type ClusterRunDto = import('../../lib/bindings').ClusterRunDto;

export interface ClusterProgressPayload {
  phase: 'loading' | 'clustering' | 'labeling' | 'saving';
  current: number;
  total: number;
}

export type QuickCaptureResultDto = import('../../lib/bindings').QuickCaptureResultDto;
