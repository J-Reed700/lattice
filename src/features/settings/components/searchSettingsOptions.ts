import type { SearchSettings as ApiSearchSettings } from '@/types/api/settings';


/// Vector storage is a property of the index rather than of a query, but it
/// lives here because this is where the index's knobs are: changing it re-keys
/// the vector space and the next start rebuilds from the vectors SQLite
/// already holds.
export type VectorCompression = ApiSearchSettings['vectorIndexCompression'];
export type CompressionMode = VectorCompression['mode'];
export type Quantization = VectorCompression['quantization'];

export const COMPRESSION_MODES: { value: CompressionMode; label: string }[] = [
  { value: 'none', label: 'Full precision' },
  { value: 'truncated', label: 'Matryoshka truncation' },
];

export const QUANTIZATIONS: { value: Quantization; label: string }[] = [
  { value: 'f32', label: '32-bit float' },
  { value: 'i8', label: '8-bit integer' },
];

/// Matches the backend's accepted range; the index clamps again against the
/// active model's real dimension, which the UI cannot know here.
export const MIN_COMPRESSION_DIMS = 32;
export const MAX_COMPRESSION_DIMS = 4096;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function toFinite(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}
