interface EntryAutosaveIndicatorProps {
  hasPendingChanges: boolean;
  isSaving: boolean;
  saveError: string | null;
  onRetry: () => void;
}

/**
 * Single quiet word indicating autosave state. No spinner, no icon.
 * Spec §5.6.
 */
export function EntryAutosaveIndicator({
  hasPendingChanges,
  isSaving,
  saveError,
  onRetry,
}: EntryAutosaveIndicatorProps) {
  if (saveError) {
    return (
      <button
        type="button"
        onClick={onRetry}
        className="text-xs text-[hsl(var(--danger-fg))] underline-offset-2 hover:underline"
        title={saveError}
      >
        Save failed — retry?
      </button>
    );
  }
  if (isSaving || hasPendingChanges) {
    return <span className="text-xs text-[hsl(var(--text-muted))]">Saving…</span>;
  }
  return <span className="text-xs text-[hsl(var(--text-muted))]">Saved</span>;
}
