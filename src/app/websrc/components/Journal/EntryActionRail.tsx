import { SynthesizePopover, type SynthesisScope } from './SynthesizePopover';

interface EntryActionRailProps {
  selectedEntryId: string | null;
  pinnedCount: number;
  deckCount: number;
  onSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  disabled?: boolean;
}

/**
 * Quiet bottom rail below the editor: Synthesize popover on the left,
 * keyboard shortcut hint on the right.
 * Spec §5.5.
 */
export function EntryActionRail({
  selectedEntryId,
  pinnedCount,
  deckCount,
  onSynthesize,
  disabled,
}: EntryActionRailProps) {
  return (
    <div className="mt-6 flex items-center justify-between border-t border-[hsl(var(--border-subtle))] pt-3">
      <SynthesizePopover
        selectedEntryId={selectedEntryId}
        pinnedCount={pinnedCount}
        deckCount={deckCount}
        onSynthesize={onSynthesize}
        disabled={disabled}
      />
      <p className="text-xs text-[hsl(var(--text-muted))]">
        Select text to highlight
      </p>
    </div>
  );
}
