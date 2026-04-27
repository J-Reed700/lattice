import type { SearchResultMetadata } from './metadata';

export interface SearchResult {
  id: string;
  title: string;
  content: string;
  highlights?: string[];
  score: number;
  path: string | null | undefined;
  documentId: string | null | undefined;
  position: number | null | undefined;
  vectorScore: number | null | undefined;
  bm25Score: number | null | undefined;
  vectorRank: number | null | undefined;
  bm25Rank: number | null | undefined;
  metadata: SearchResultMetadata | any;
}
