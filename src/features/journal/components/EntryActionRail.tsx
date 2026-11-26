import {
  SynthesizePopover,
  type SynthesisScope,
  type WeekCandidateCounts,
} from '@/features/journal/components/SynthesizePopover';

interface EntryActionRailProps {
  selectedEntryId: string | null;
  pinnedCount: number;
  deckCount: number;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  disabled?: boolean;
  weekCandidates?: WeekCandidateCounts;
}

/**
 * Calm editorial writing bar beneath the page: Synthesize popover on the left,
 * subtle highlight hint on the right.
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
    <div className="flex items-center justify-between gap-4">
      <SynthesizePopover
        selectedEntryId={selectedEntryId}
        pinnedCount={pinnedCount}
        deckCount={deckCount}
        onSynthesize={onSynthesize}
        disabled={disabled}
        weekCandidates={weekCandidates}
      />
      <p className="text-xs text-text-muted">
        Select text to highlight
      </p>
    </div>
  );
}
