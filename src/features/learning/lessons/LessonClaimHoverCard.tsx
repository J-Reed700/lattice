import { CircleCheck, CircleDashed, CircleHelp, CircleX } from 'lucide-react';

import { checkWords, claimMarkVerdict, partsOfKind, resolutionText, UNSTATED_PARTS } from '@/features/learning/model/evidencePresentation';
import { relationLabel } from '@/features/reading/citations';
import { SentenceHoverCard, type ClaimHover } from '@/features/reading/components/SentenceHoverCard';
import type { LearningCitationDto, LearningClaimEvidenceDto } from '@/lib/bindings';
import { sanitizeFileName } from '@/utils/sanitize';

const HEADLINE = {
  supported: { icon: CircleCheck, tone: 'text-[hsl(var(--success-fg))]' },
  contradicted: { icon: CircleX, tone: 'text-[hsl(var(--danger-fg))]' },
  unsupported: { icon: CircleHelp, tone: 'text-[hsl(var(--warning-fg))]' },
  unverified: { icon: CircleDashed, tone: 'text-text-muted' },
} as const;

/** Why a sentence of the lesson is or is not trusted, in the lesson's own words, while the pointer rests on it. */
export function LessonClaimHoverCard({ hover, claim, citations }: {
  hover: ClaimHover | null;
  claim: LearningClaimEvidenceDto | null;
  citations: ReadonlyMap<number, LearningCitationDto>;
}) {
  if (!hover || !claim) return null;

  const { icon: Icon, tone } = HEADLINE[claimMarkVerdict(claim)];
  const how = checkWords(claim, citations);
  const cited = (claim.cites ?? []).flatMap((number) => {
    const citation = citations.get(number);
    return citation ? [{ number, title: citation.passage.title }] : [];
  });

  return (
    <SentenceHoverCard hover={hover}>
      <p className={`flex items-center gap-1.5 text-ui font-medium ${tone}`}>
        <Icon className="h-3.5 w-3.5 shrink-0" strokeWidth={1.8} aria-hidden="true" />
        {resolutionText(claim)}
      </p>
      {claim.relation && <p className="mt-0.5 pl-5 text-xs text-text-muted">{relationLabel(claim.relation)}</p>}
      {how && <p className="mt-0.5 pl-5 text-xs text-text-muted">{how}</p>}
      {claim.resolution === 'not_verified' && claim.reason && (
        <p className="mt-2 text-xs leading-relaxed text-text-secondary">{claim.reason}</p>
      )}
      {claim.resolution === 'confirmed_by_you' && claim.note && (
        <p className="mt-2 text-xs leading-relaxed text-text-secondary">Your note: {claim.note}</p>
      )}
      {UNSTATED_PARTS.map(({ kind, label }) => {
        const parts = partsOfKind(claim, kind);
        return parts.length > 0 && (
          <p key={kind} className="mt-2 text-xs leading-relaxed text-text-secondary"><span className="font-medium text-text-primary">{label}</span>{parts.join('; ')}</p>
        );
      })}
      {cited.length > 0 && (
        <ul className="mt-2.5 space-y-1">
          {cited.map(({ number, title }) => (
            <li key={number} className="flex items-center gap-2 text-xs text-text-secondary">
              <span className="flex h-4 min-w-4 items-center justify-center rounded-[4px] bg-accent-muted px-1 text-[10px] font-semibold tabular-nums text-accent">
                {number}
              </span>
              <span className="truncate">{sanitizeFileName(title)}</span>
            </li>
          ))}
        </ul>
      )}
    </SentenceHoverCard>
  );
}
