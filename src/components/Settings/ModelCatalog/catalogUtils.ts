/**
 * Shared vocabulary for the catalog: how a model is described in one line,
 * and what counts as an active filter. Kept in one place so the filter row,
 * the result rows, and the empty state all agree.
 */

import type { SystemCapabilities } from '../../../types/api/models';
import type { ModelCategory, ModelMetadata, SearchFilters } from '../../../types/modelCatalog';

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
  return sizeGb < 1 ? `${Math.round(sizeGb * 1024)} MB` : `${sizeGb.toFixed(1)} GB`;
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

  const quantization = metadata.supported_quantizations[0];
  if (quantization) parts.push(quantization);

  if (metadata.format === 'safetensors') parts.push('Safetensors');

  if (metadata.requires_auth) parts.push('Token required');

  return parts.join(' · ');
}

/** The reason an embedding model can't be loaded here, or null if it can. */
export function embeddingBlockReason(metadata: ModelMetadata): string | null {
  if (metadata.category !== 'Embedding') return null;
  const compatibility = metadata.embedding_compatibility;
  if (!compatibility || compatibility.kind === 'compatible') return null;
  return compatibility.kind === 'incompatible'
    ? compatibility.reason
    : 'Architecture not recognized — only BERT-family embedders are supported today.';
}

export type ModelFitVerdict = 'fits' | 'tight' | 'too-large';

export interface ModelFit {
  verdict: ModelFitVerdict;
  /** One line for the tooltip: "Needs 8 GB · 22 GB usable". */
  reason: string;
  /** Memory the model may actually use, in GB. Exposed for tests and the detail panel. */
  usableMemoryGb: number;
}

export const FIT_LABEL: Record<ModelFitVerdict, string> = {
  fits: 'Fits',
  tight: 'Tight',
  'too-large': 'Too large',
};

/**
 * "8", "13.5", "22.4" — at most one decimal, and never a trailing ".0".
 * A model that needs 8 GB should not read "Needs 8.0 GB".
 */
function formatGb(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1);
}

/**
 * One word about whether a model will run on this machine.
 *
 * Usable memory:
 *  - Apple Silicon is unified memory — the GPU allocates out of system RAM, so
 *    there is no second pool. `vram_gb` is `null` on that platform today; we
 *    ignore it even if a future probe reports one, and budget 70% of total RAM,
 *    leaving the OS and the rest of the app the other 30%.
 *  - A discrete GPU with acceleration and a known positive VRAM figure: the
 *    VRAM, because that is the pool the model is loaded into.
 *  - Everything else (no acceleration, unknown VRAM): 70% of total RAM.
 *
 * Returns null — no word at all — when the machine or the model's requirement
 * is unknown. A guess dressed as a verdict is worse than silence.
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

  // Disk gate. A failed disk probe (0 or non-finite) skips the gate rather than
  // condemning every model on the machine.
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
