import { BadgeCheck, Ban, CircleHelp, Landmark, UserCheck, Users, type LucideIcon } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { badgeText } from '@/features/learning/sources/trust/trustPresentation';
import { TrustReasons } from '@/features/learning/sources/trust/TrustReasons';
import type { LearningSourceTrustDto, LearningSourceTrustEntryDto, LearningTrustLevel } from '@/lib/bindings';

/** A distinct shape per level, so the level never rests on colour alone. */
const LEVEL_ICON: Record<LearningTrustLevel, LucideIcon> = {
  primary: BadgeCheck,
  authoritative: Landmark,
  established: UserCheck,
  community: Users,
  unknown: CircleHelp,
  distrusted: Ban,
};

const CHIP = 'inline-flex max-w-full items-center gap-1 rounded-full border border-border px-2 py-0.5 text-[10px] font-medium text-text-secondary';

/** The level as words and a shape. Not interactive: safe inside a row button or a summary. */
export function TrustLevelChip({ trust }: { trust: LearningSourceTrustDto }) {
  const Icon = LEVEL_ICON[trust.level];
  return (
    <span className={CHIP} data-trust-level={trust.level}>
      <Icon size={12} aria-hidden="true" className="shrink-0" />
      <span className="truncate">{badgeText(trust)}</span>
    </span>
  );
}

/**
 * The level badge (§19). Its button opens the reasons, verbatim, with the
 * publisher, dates, relations, track record and overrides when known.
 */
export function TrustBadge({
  trust,
  entry,
  titleOf,
}: {
  trust: LearningSourceTrustDto;
  /** The source's full trust entry, for the facts beside the reasons. */
  entry?: LearningSourceTrustEntryDto | null;
  /** A saved version's title, for relation lines. */
  titleOf?: (versionId: string) => string | null;
}) {
  const Icon = LEVEL_ICON[trust.level];
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          data-trust-level={trust.level}
          className={`${CHIP} min-h-6 hover:border-accent/50 hover:text-text-primary focus-visible:outline-2 focus-visible:outline-accent`}
        >
          <Icon size={12} aria-hidden="true" className="shrink-0" />
          <span className="truncate">{badgeText(trust)}</span>{' '}
          <span className="text-text-muted">· Why?</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="max-h-96 w-80 overflow-y-auto">
        <TrustReasons trust={trust} entry={entry ?? null} titleOf={titleOf} />
      </PopoverContent>
    </Popover>
  );
}
