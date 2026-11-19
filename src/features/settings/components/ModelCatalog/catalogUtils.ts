/**
 * Shared vocabulary for the catalog: how a model is described in one line,
 * and what counts as an active filter. Kept in one place so the filter row,
 * the result rows, and the empty state all agree.
 */


import type { ModelCategory, ModelMetadata, SearchFilters } from '@/types/modelCatalog';

import { modelQuantization } from './quantization';

/**
 * The "Reset filters" affordance. A text button with no padding so it sits
 * flush with the controls it follows and with the text it sits beside.
 */
export const CATALOG_TEXT_BUTTON_CLASS =
  'inline-flex h-8 shrink-0 items-center text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary';

/** "LLM" is the backend's word. The user's word is "Chat". */
export const CATEGORY_LABELS: Record<ModelCategory, string> = {
  LLM: 'Chat',
  Embedding: 'Embedding',
  OCR: 'OCR',
  Transcription: 'Transcription',
  Speech: 'Read aloud',
};

export function hasActiveFilters(filters: SearchFilters): boolean {
  return (
    filters.category !== null ||
    filters.max_size_gb !== null ||
    filters.min_downloads !== null ||
    filters.required_capabilities.length > 0 ||
    filters.embedding_dimensions !== null
  );
}

/** Everything cleared except the search text, which the search field owns. */
export function clearedFilters(filters: SearchFilters): SearchFilters {
  return {
    category: null,
    max_size_gb: null,
    min_downloads: null,
    required_capabilities: [],
    query_text: filters.query_text,
    embedding_dimensions: null,
  };
}

export function formatSize(sizeGb: number): string | null {
  if (!sizeGb || sizeGb <= 0) return null;
  return sizeGb < 1 ? `${Math.round(sizeGb * 1000)} MB` : `${sizeGb.toFixed(1)} GB`;
}

export function formatCompact(value: number | null | undefined): string | null {
  if (value == null || value <= 0) return null;
  return new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(
    value,
  );
}

/**
 * The one muted line under a model's name: size, downloads, category, and
 * whatever else this particular model actually has. Absent fields are absent
 * rather than rendered as "Unknown".
 */
export function modelMetaLine(
  metadata: ModelMetadata,
  popularityDownloads: number | null | undefined,
): string {
  const parts: string[] = [];

  const size = formatSize(metadata.size_gb);
  if (size) parts.push(size);

  const downloads = formatCompact(popularityDownloads);
  if (downloads) parts.push(`${downloads} downloads`);

  parts.push(CATEGORY_LABELS[metadata.category]);

  if (metadata.category === 'Embedding' && metadata.embedding_dimensions) {
    parts.push(`${metadata.embedding_dimensions} dimensions`);
  }

  const quantization = modelQuantization(metadata);
  if (quantization) parts.push(quantization);

  if (metadata.requires_auth) parts.push('Token required');

  return parts.join(' · ');
}

/** The reason an embedding model can't be loaded here, or null if it can. */
export function embeddingBlockReason(metadata: Pick<ModelMetadata, 'category' | 'embedding_compatibility'>): string | null {
  if (metadata.category !== 'Embedding') return null;
  const compatibility = metadata.embedding_compatibility;
  if (!compatibility || compatibility.kind === 'compatible') return null;
  return compatibility.kind === 'incompatible'
    ? compatibility.reason
    : 'This architecture is not recognized by the local embedding runtime.';
}
