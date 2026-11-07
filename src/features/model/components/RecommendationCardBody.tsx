import type { RecommendationDto, RecommendationLabelDto, SpeedClassDto } from '@/lib/bindings';

import { SeesImagesMark } from './SeesImagesMark';
import { formatGib, fitUsage } from '../utils/fitText';

const LABEL_TEXT: Record<RecommendationLabelDto, string> = {
  best: 'Best for this computer',
  fastest: 'Fastest',
  smallest_download: 'Smallest download',
};

const SPEED_TEXT: Record<SpeedClassDto, string> = {
  fast: 'Fast',
  balanced: 'Medium',
  thorough: 'Slower',
};

const BADGE_CLASS = 'rounded-md bg-accent-muted px-1.5 py-0.5 text-[11px] font-medium text-accent';

function downloadSize(bytes: number): string {
  return bytes >= 1_073_741_824 ? `${formatGib(bytes)} GB` : `${Math.round(bytes / 1_048_576)} MB`;
}

interface RecommendationCardBodyProps {
  recommendation: RecommendationDto;
  seesImages?: boolean;
}

/** One recommended model: what it is, what it costs here, and what Lattice knows about it. */
export function RecommendationCardBody({ recommendation, seesImages = false }: RecommendationCardBodyProps) {
  const { verdict, labels, evidence } = recommendation;
  return (
    <span className="block min-w-0 flex-1 space-y-1">
      <span className="flex items-baseline justify-between gap-3">
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium text-text-primary">{recommendation.name}</span>
          {seesImages ? <SeesImagesMark /> : null}
        </span>
        <span className="shrink-0 whitespace-nowrap text-xs tabular-nums text-text-secondary">
          {downloadSize(recommendation.downloadBytes)}
        </span>
      </span>
      {labels.length > 0 || evidence.kind === 'measured' ? (
        <span className="flex flex-wrap gap-1.5">
          {labels.map((label) => <span key={label} className={BADGE_CLASS}>{LABEL_TEXT[label]}</span>)}
          {evidence.kind === 'measured' ? <span className={BADGE_CLASS}>Tested by Lattice</span> : null}
        </span>
      ) : null}
      <span className="block text-xs text-text-primary">{recommendation.useFor}</span>
      <span className="block text-xs text-text-muted">{recommendation.description}</span>
      <span className="block text-xs tabular-nums text-text-muted">{fitUsage(verdict)}</span>
      {verdict.device.kind === 'processor' ? (
        <span className="block text-xs text-warning-fg">Runs on the processor: slower</span>
      ) : null}
      <span className="block text-xs text-text-muted">Speed: {SPEED_TEXT[recommendation.speed]}</span>
    </span>
  );
}
