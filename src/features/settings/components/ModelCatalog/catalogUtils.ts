import type { SystemCapabilities } from '@/types/api/models';
import type { ModelCategory, ModelMetadata, SearchFilters } from '@/types/modelCatalog';

import { modelQuantization } from './quantization';

export const CATALOG_TEXT_BUTTON_CLASS =
  'inline-flex h-8 shrink-0 items-center text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary';

export const CATEGORY_LABELS: Record<ModelCategory, string> = {
  LLM: 'Chat',
  Embedding: 'Embedding',
  OCR: 'OCR',
  Transcription: 'Transcription',
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

  if (metadata.format === 'safetensors') parts.push('Safetensors');

  if (metadata.requires_auth) parts.push('Token required');

  return parts.join(' · ');
}

/** The reason an embedding model can't be loaded, or null if compatible. */
export function embeddingBlockReason(metadata: ModelMetadata): string | null {
  if (metadata.category !== 'Embedding') return null;
  const compatibility = metadata.embedding_compatibility;
  if (!compatibility || compatibility.kind === 'compatible') return null;
  return compatibility.kind === 'incompatible'
    ? compatibility.reason
    : 'This architecture is not recognized by the local embedding runtime.';
}

export type ModelFitVerdict = 'fits' | 'tight' | 'too-large';

export interface ModelFit {
  verdict: ModelFitVerdict;
  reason: string;
  usableMemoryGb: number;
}

export const FIT_LABEL: Record<ModelFitVerdict, string> = {
  fits: 'Fits',
  tight: 'Tight',
  'too-large': 'Too large',
};

function formatGb(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1);
}

/**
 * Computes model compatibility verdict based on system RAM/VRAM and available disk.
 * - Apple Silicon unified memory: budgets 70% of total RAM.
 * - Discrete GPU with positive VRAM: budgets VRAM pool.
 * - Fallback: budgets 70% of total RAM.
 */
export function computeModelFit(
  model: Pick<ModelMetadata, 'minimum_ram_gb' | 'size_gb'>,
  capabilities: SystemCapabilities | null | undefined,
): ModelFit | null {
  if (capabilities == null) return null;

  const totalRam = capabilities.total_ram_gb;
  if (!Number.isFinite(totalRam) || totalRam <= 0) return null;

  const needed = model.minimum_ram_gb;
  if (!Number.isFinite(needed) || needed <= 0) return null;

  const vram = capabilities.vram_gb;
  const usable =
    capabilities.gpu_type === 'AppleSilicon'
      ? 0.7 * totalRam
      : capabilities.gpu_acceleration !== 'None' && vram != null && vram > 0
        ? vram
        : 0.7 * totalRam;

  // Disk gate: skip check if disk probe is unavailable.
  const freeDisk = capabilities.available_disk_gb;
  const sizeGb = model.size_gb;
  if (
    Number.isFinite(sizeGb) &&
    sizeGb > 0 &&
    Number.isFinite(freeDisk) &&
    freeDisk > 0 &&
    sizeGb > freeDisk
  ) {
    return {
      verdict: 'too-large',
      reason: `Needs ${formatGb(sizeGb)} GB on disk · ${formatGb(freeDisk)} GB free`,
      usableMemoryGb: usable,
    };
  }

  const reason = `Needs ${formatGb(needed)} GB · ${formatGb(usable)} GB usable`;

  if (needed <= 0.8 * usable) return { verdict: 'fits', reason, usableMemoryGb: usable };
  if (needed <= usable) return { verdict: 'tight', reason, usableMemoryGb: usable };
  return { verdict: 'too-large', reason, usableMemoryGb: usable };
}
