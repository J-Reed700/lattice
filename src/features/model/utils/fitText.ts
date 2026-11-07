import type { FitKindDto, FitVerdictDto } from '@/lib/bindings';

const GIB = 1_073_741_824;

export const FIT_LABEL: Record<FitKindDto, string> = {
  fits: 'Fits',
  tight: 'Fits with a shorter context',
  too_large: 'Too large for this computer',
};

/** "8", "13.5", "16": at most one decimal, never a trailing ".0". */
export function formatGib(bytes: number): string {
  const rounded = Math.round((bytes / GIB) * 10) / 10;
  return Number.isInteger(rounded) ? rounded.toString() : rounded.toFixed(1);
}

/** The backend's verdict in words; `null` is a size being read, `'unknown'` one that could not be. */
export function fitLabel(fit: FitVerdictDto | 'unknown' | null): string {
  if (fit === 'unknown') return 'Size unknown';
  return fit ? FIT_LABEL[fit.kind] : 'Checking size';
}

/** "Uses 11.2 of 16 GB on NVIDIA GeForce RTX 4080": the device's whole memory, as its owner knows it. */
export function fitUsage(fit: FitVerdictDto): string {
  return `Uses ${formatGib(fit.needsBytes)} of ${formatGib(fit.device.totalBytes)} GB on ${fit.device.name}`;
}
