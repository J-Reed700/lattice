/**
 * Corpus API Types (Track B)
 *
 * Shapes for the vault-wide readouts: type mix, growth, document neighbourhoods
 * and the automatic themes produced by `corpus_shape`.
 */

export interface CorpusTypeCountDto {
  type: string;
  count: number;
}

export interface CorpusShapeDto {
  total: number;
  byType: CorpusTypeCountDto[];
  grownLast7Days: number;
}

export interface CitingConversationDto {
  conversationId: string;
  title: string;
  updatedAt: string;
  passageCount: number;
}

export interface SimilarDocumentDto {
  documentId: string;
  title: string;
  filePath: string | null;
  score: number;
}

export type ClusterLabelSource = 'llm' | 'inherited_exact' | 'inherited_jaccard' | 'fallback';

/**
 * One automatic theme. Called a "cluster" on the backend (the table and the
 * commands predate the name); the UI says "theme" everywhere.
 */
export interface ClusterDto {
  id: string;
  label: string;
  description: string | null;
  memberCount: number;
  sampleTitles: string[];
  labelSource: ClusterLabelSource;
  inheritedFromClusterId: string | null;
  memberDocumentIds: string[];
}

export interface ClusterRunDto {
  runId: string;
  ranAt: string;
  docCount: number;
  clusterCount: number;
  noiseCount: number;
  durationMs: number;
  llmCalls: number;
}

export interface ClusterProgressPayload {
  phase: 'loading' | 'clustering' | 'labeling' | 'saving';
  current: number;
  total: number;
}

export interface QuickCaptureResultDto {
  noteId: string;
  noteTitle: string;
  created: boolean;
}
