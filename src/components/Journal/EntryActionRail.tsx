import {
  SynthesizePopover,
  type SynthesisScope,
  type WeekCandidateCounts,
} from './SynthesizePopover';

interface EntryActionRailProps {
  selectedEntryId: string | null;
  pinnedCount: number;
  deckCount: number;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  disabled?: boolean;
  weekCandidates?: WeekCandidateCounts;
}

/**
 * Foot of the context rail: Synthesize popover on the left, the highlight
 * hint on the right.
 * Spec §5.5.
 */
export function EntryActionRail({
  selectedEntryId,
  pinnedCount,
  deckCount,
  onSynthesize,
  disabled,
  weekCandidates,
}: EntryActionRailProps) {
  return (
    <div className="flex items-center justify-between gap-3">
      <SynthesizePopover
        selectedEntryId={selectedEntryId}
        pinnedCount={pinnedCount}
        deckCount={deckCount}
        onSynthesize={onSynthesize}
        disabled={disabled}
        weekCandidates={weekCandidates}
      />
      <p className="text-[11px] text-[hsl(var(--text-muted))]">
        Select text to highlight
      </p>
    </div>
  );
}
