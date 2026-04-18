/**
 * Shared constants and utilities for AI settings sub-tabs.
 */

import type {
  CustomToolSettings as ApiCustomToolSettings,
} from '../../../types/api/settings';

export type CustomToolPreset = {
  id: string;
  label: string;
  summary: string;
  docsUrl?: string;
  tool: ApiCustomToolSettings;
};

export const CUSTOM_TOOL_PRESETS: CustomToolPreset[] = [
  {
    id: 'searxng_search',
    label: 'SearXNG (self-hosted)',
    summary: 'Meta-search through your own SearXNG instance (best for privacy and control)',
    docsUrl: 'https://docs.searxng.org/dev/search_api.html',
    tool: {
      enabled: true,
      name: 'searxng_search',
      description: 'Search with your SearXNG instance (JSON endpoint)',
      endpoint: 'https://your-searxng-domain.example/search?format=json',
      queryParam: 'q',
      maxResultsParam: null,
      defaultMaxResults: 5,
    },
  },
  {
    id: 'openalex_search',
    label: 'OpenAlex (works)',
    summary: 'Scholarly works metadata and abstracts index',
    docsUrl: 'https://docs.openalex.org/',
    tool: {
      enabled: true,
      name: 'openalex_search',
      description: 'Search scholarly works via OpenAlex (free, no API key)',
      endpoint: 'https://api.openalex.org/works',
      queryParam: 'search',
      maxResultsParam: 'per-page',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'crossref_works',
    label: 'Crossref (works)',
    summary: 'Publication metadata and DOI discovery',
    docsUrl: 'https://api.crossref.org/swagger-ui/index.html',
    tool: {
      enabled: true,
      name: 'crossref_works',
      description: 'Search publication metadata via Crossref (free, no API key)',
      endpoint: 'https://api.crossref.org/works',
      queryParam: 'query',
      maxResultsParam: 'rows',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'europe_pmc_search',
    label: 'Europe PMC',
    summary: 'Biomedical literature and preprints',
    docsUrl: 'https://europepmc.org/RestfulWebService',
    tool: {
      enabled: true,
      name: 'europe_pmc_search',
      description: 'Search biomedical literature via Europe PMC (free, no API key)',
      endpoint: 'https://www.ebi.ac.uk/europepmc/webservices/rest/search?format=json',
      queryParam: 'query',
      maxResultsParam: 'pageSize',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'openlibrary_search',
    label: 'Open Library',
    summary: 'Books, editions, and bibliographic records',
    docsUrl: 'https://openlibrary.org/dev/docs/api/search',
    tool: {
      enabled: true,
      name: 'openlibrary_search',
      description: 'Search books and editions via Open Library (free, no API key)',
      endpoint: 'https://openlibrary.org/search.json',
      queryParam: 'q',
      maxResultsParam: 'limit',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'semantic_scholar_search',
    label: 'Semantic Scholar',
    summary: 'Academic paper search with paper metadata',
    docsUrl: 'https://api.semanticscholar.org/api-docs/',
    tool: {
      enabled: true,
      name: 'semantic_scholar_search',
      description: 'Search papers via Semantic Scholar Graph API (free tier, no key required)',
      endpoint: 'https://api.semanticscholar.org/graph/v1/paper/search',
      queryParam: 'query',
      maxResultsParam: 'limit',
      defaultMaxResults: 10,
    },
  },
];

export const INPUT_CLASS =
  'w-full px-3 py-2 text-sm bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] border border-[hsl(var(--border-subtle))] rounded-lg focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))] focus:border-transparent';

export const TEXTAREA_CLASS =
  'w-full rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] px-3 py-2 text-xs text-[hsl(var(--text-primary))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))] focus:border-transparent';

export const toFinite = (value: string, fallback: number) => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
};

export const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

export const normalizeCustomTool = (tool: ApiCustomToolSettings): ApiCustomToolSettings => {
  const maxResults = Number.isFinite(tool.defaultMaxResults)
    ? Math.max(1, Math.min(100, Math.round(tool.defaultMaxResults)))
    : 5;
  const maxResultsParam = (tool.maxResultsParam || '').trim();

  return {
    enabled: tool.enabled,
    name: tool.name.trim(),
    description: tool.description.trim(),
    endpoint: tool.endpoint.trim(),
    queryParam: tool.queryParam.trim(),
    maxResultsParam: maxResultsParam.length > 0 ? maxResultsParam : null,
    defaultMaxResults: maxResults,
  };
};
