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
  searchDocuments: async (
    options: SearchOptions,
  ): Promise<ApiResult<SearchResult[]>> => {
    const raw = await apiCall<unknown>('search_documents', { options });
    return unwrapSearchResults(raw, 'Document search failed');
  },

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

  /** Finds documents whose content is closest to a given document ID. */
  findSimilarDocuments: async (
    documentId: string,
    limit?: number,
  ): Promise<ApiResult<SimilarDocumentDto[]>> =>
    apiCall<Wire.SimilarDocumentDto[]>('find_similar_documents', {
      documentId,
      limit,
    }),

  /** Whether reranking is enabled and its model is downloaded. */
  getRerankerStatus: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('reranker_status'),

  /** Downloads the reranker model without changing the enabled setting. */
  downloadReranker: async (): Promise<ApiResult<Wire.RerankerStatusDto>> =>
    apiCall<Wire.RerankerStatusDto>('download_reranker'),
};
