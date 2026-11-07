import type { CodeReviewSeverity } from '@/lib/bindings';

/** Every severity, the most severe first. */
export const SEVERITIES: readonly CodeReviewSeverity[] = ['critical', 'major', 'minor', 'nit'];

const LABELS: Record<CodeReviewSeverity, string> = {
  critical: 'Critical',
  major: 'Major',
  minor: 'Minor',
  nit: 'Nit',
};

const TONES: Record<CodeReviewSeverity, string> = {
  critical: 'bg-danger-muted text-danger-fg',
  major: 'bg-warning-muted text-warning-fg',
  minor: 'bg-accent-muted text-accent',
  nit: 'bg-surface-raised text-text-secondary',
};

export function severityLabel(severity: CodeReviewSeverity): string {
  return LABELS[severity];
}

/** The classes of a severity's badge: the shape of a claim review's state chip in the severity's colour. */
export function severityTone(severity: CodeReviewSeverity): string {
  return `whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-semibold ${TONES[severity]}`;
}
