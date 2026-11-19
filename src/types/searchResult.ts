import type { SearchResultMetadata } from './metadata';
import type { SearchResultDto } from '../lib/bindings';

/** `metadata` is sent but skipped by the generator; `highlights` is added by the UI. */
export type SearchResult = SearchResultDto & {
  highlights?: string[];
  metadata: Partial<SearchResultMetadata> & Record<string, unknown>;
};
