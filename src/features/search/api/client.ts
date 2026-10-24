import type * as Wire from '@/lib/bindings';
import { apiCall, unwrapNestedApiResult } from '@/shared/ipc/transport';
import type {
  SearchOptions,
  SearchResult,
  ApiResult,
  SimilarDocumentDto,
} from '@/types';

function normalizeSearchResult(raw: unknown): SearchResult | null {
  if (
    !isRecord(raw) ||
    typeof raw.id !== 'string' ||
    !raw.id ||
    typeof raw.title !== 'string' ||
    typeof raw.content !== 'string' ||
    typeof raw.score !== 'number' ||
    !Number.isFinite(raw.score)
  ) {
    return null;
  }

  const { id, title, content, score } = raw;

  return {
    id,
    title,
    content,
    score,
    path: (raw.path as string | null | undefined) ?? null,
    documentId:
      (raw.documentId as string | null | undefined) ??
      (raw.document_id as string | null | undefined) ??
      null,
    position: (raw.position as number | null | undefined) ?? null,
    vectorScore:
      (raw.vectorScore as number | null | undefined) ??
      (raw.vector_score as number | null | undefined) ??
      null,
    bm25Score:
      (raw.bm25Score as number | null | undefined) ??
      (raw.bm25_score as number | null | undefined) ??
      null,
    vectorRank:
      (raw.vectorRank as number | null | undefined) ??
      (raw.vector_rank as number | null | undefined) ??
      null,
    bm25Rank:
      (raw.bm25Rank as number | null | undefined) ??
      (raw.bm25_rank as number | null | undefined) ??
      null,
    metadata: (raw.metadata as Record<string, unknown> | undefined) ?? {},
    highlights: Array.isArray(raw.highlights)
      ? (raw.highlights as string[])
      : undefined,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function extractSearchResults(payload: unknown): SearchResult[] | null {
  if (Array.isArray(payload)) {
    const results = payload.map(normalizeSearchResult);
    return results.every((item): item is SearchResult => item !== null)
      ? results
      : null;
  }

  if (!isRecord(payload)) {
    return null;
  }

  if (payload.ok === true && 'data' in payload) {
    return extractSearchResults(payload.data);
  }

  if (Array.isArray(payload.results)) {
    return extractSearchResults(payload.results);
  }

  if (Array.isArray(payload.data)) {
    return extractSearchResults(payload.data);
  }

  return null;
}

function unwrapSearchResults(
  response: ApiResult<unknown>,
  fallbackError: string,
): ApiResult<SearchResult[]> {
  const unwrapped = unwrapNestedApiResult<unknown>(response, fallbackError);
  if (!unwrapped.ok) {
    return unwrapped;
  }

  const data = extractSearchResults(unwrapped.data);
  return data === null
    ? { ok: false, error: `${fallbackError}: invalid search response` }
    : { ok: true, data };
}
export const searchApi = {
  /**
   * Searches indexed documents using configurable search options.
   * Supports semantic search via embeddings, with optional filters and ranking.
   *
   * @param options - Search configuration including query, limit, filters, and mode
   * @returns Array of search results with content, metadata, and similarity scores
   *
   * @example
   * const result = await VaultAPI.searchDocuments({
   *   query: 'machine learning',
   *   limit: 10,
   *   filters: { tags: ['ai'] }
   * });
   */
  searchDocuments: async (
    options: SearchOptions,
  ): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_documents', { options });
    return unwrapSearchResults(raw, 'Document search failed');
  },

  /**
   * Performs recency-aware search that boosts recent documents.
   * Combines relevance scoring with time-based weighting.
   *
   * @param options - Recency search options with query, limit, weights
   * @returns Search results with recency boost applied
   *
   * @example
   * const result = await VaultAPI.searchWithRecency({
   *   query: 'recent updates',
   *   limit: 10,
   *   recencyWeight: 0.3,
   *   maxAgeDays: 30
   * });
   */
  searchWithRecency: async (options: {
    query: string;
    limit?: number;
    recencyWeight?: number;
    maxAgeDays?: number;
  }): Promise<ApiResult<SearchResult[]>> => {
    const result = await apiCall<Wire.SearchResultDto[]>(
      'search_with_recency',
      { options },
    );
    return unwrapSearchResults(result, 'Recency search failed');
  },

  /**
   * Performs batch search for multiple queries efficiently.
   * Processes all queries in parallel and returns results in order.
   *
   * @param queries - Array of search query strings
   * @param limit - Maximum results per query (default: 10)
   * @param searchMode - Search algorithm: 'vector', 'keyword', or 'hybrid'
   * @returns Array of result arrays, one for each query
   *
   * @example
   * const results = await VaultAPI.batchSearch(
   *   ['machine learning', 'neural networks', 'deep learning'],
   *   5,
   *   'hybrid'
   * );
   * // results[0] = results for 'machine learning'
   * // results[1] = results for 'neural networks'
   * // results[2] = results for 'deep learning'
   */
  batchSearch: async (
    queries: string[],
    limit?: number,
    searchMode?: string,
  ): Promise<ApiResult<SearchResult[][]>> => {
    const result = await apiCall<Wire.SearchResultDto[][]>('batch_search', {
      queries,
      limit,
      searchMode,
    });
    if (!result.ok) return result;
    const groups: SearchResult[][] = [];
    for (const raw of result.data) {
      const group = unwrapSearchResults(
        { ok: true, data: raw },
        'Batch search failed',
      );
      if (!group.ok) return group;
      groups.push(group.data);
    }
    return { ok: true, data: groups };
  },

  /**
   * Performs fast full-text search without semantic embeddings.
   * Uses SQLite FTS5 for keyword matching. Ideal for simple queries.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of matching documents ranked by relevance
   */
  searchFast: async (
    query: string,
    limit?: number,
  ): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_fast', {
      query,
      limit: limit ?? 10,
    });
    return unwrapSearchResults(raw, 'Keyword search failed');
  },

  /**
   * Executes hybrid search combining semantic and keyword matching.
   * Merges results from embeddings and FTS5, reranked by combined score.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (optional)
   * @param searchMode - Search strategy: 'semantic', 'keyword', or 'hybrid' (default)
   * @returns Unified array of search results with combined relevance scores
   */
  searchHybrid: async (
    query: string,
    limit?: number,
    searchMode?: 'semantic' | 'keyword' | 'hybrid',
  ): Promise<ApiResult<SearchResult[]>> => {
    const effectiveLimit = limit ?? 10;
    const effectiveSearchMode = searchMode ?? 'hybrid';
    const raw = await apiCall<unknown>('hybrid_search', {
      args: {
        query,
        limit: effectiveLimit,
        search_mode: effectiveSearchMode,
      },
    });
    const primary = unwrapSearchResults(raw, 'Hybrid search failed');
    if (!primary.ok || primary.data.length > 0) {
      return primary;
    }

    // Fallback: use the unified search command path if the direct hybrid command
    // unexpectedly yields an empty payload shape.
    const fallbackRaw = await apiCall<unknown>('search_documents', {
      options: {
        query,
        limit: effectiveLimit,
        searchMode: effectiveSearchMode,
      },
    });
    return unwrapSearchResults(fallbackRaw, 'Hybrid search failed');
  },

  /**
   * Performs pure semantic vector similarity search.
   * Uses embeddings to find semantically similar documents without keyword matching.
   *
   * @param query - Search query text
   * @param limit - Maximum number of results to return (default: 10)
   * @returns Array of search results ranked by semantic similarity
   */
  searchSemantic: async (
    query: string,
    limit?: number,
  ): Promise<ApiResult<SearchResult[]>> => {
    const request: Wire.SearchRequestDto = {
      query,
      limit: limit ?? 10,
      threshold: null,
      mode: { type: 'vector' },
    };
    const raw = await apiCall<unknown>('semantic_search', { request });
    const primary = unwrapSearchResults(raw, 'Semantic search failed');
    if (!primary.ok || primary.data.length > 0) {
      return primary;
    }

    const fallbackRaw = await apiCall<unknown>('search_documents', {
      options: {
        query,
        limit: limit || 10,
        searchMode: 'semantic',
      },
    });
    return unwrapSearchResults(fallbackRaw, 'Semantic search failed');
  },

  /**
   * Finds documents similar to a given document by chunk ID.
   * Uses vector similarity to find related content.
   *
   * @param chunkId - ID of the chunk to find similar documents for
   * @param limit - Maximum number of results to return (optional)
   * @returns Array of similar documents ranked by similarity score
   */
  findSimilar: async (
    chunkId: string,
    limit?: number,
  ): Promise<ApiResult<SearchResult[]>> => {
    const result = await apiCall<Wire.SearchResultDto[]>('find_similar', {
      chunkId,
      limit,
    });
    return unwrapSearchResults(result, 'Similar document search failed');
  },

  /**
   * Finds documents whose content is closest to a given document.
   *
   * Document-shaped where `findSimilar` is chunk-shaped: the backend picks a
   * representative chunk, searches, and collapses the hits to one row per
   * document, so the caller never has to know a document's chunk ids.
   *
   * @param documentId - The document to find neighbours for
   * @param limit - Maximum number of documents to return (default 6)
   */
  findSimilarDocuments: async (
    documentId: string,
    limit?: number,
  ): Promise<ApiResult<SimilarDocumentDto[]>> =>
    apiCall<Wire.SimilarDocumentDto[]>('find_similar_documents', {
      documentId,
      limit,
    }),

  /**
   * Whether reranking is switched on, and whether its model is on disk.
   *
   * The two are independent: the setting can be on with nothing installed, in
   * which case search silently returns its unreranked shortlist. `active` is
   * the only field that says what will actually happen.
   */
  getRerankerStatus: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('reranker_status'),

  /**
   * Fetches the reranker model. Does not switch reranking on — acquiring the
   * model and choosing to use it stay separate decisions.
   */
  downloadReranker: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('download_reranker'),
};
