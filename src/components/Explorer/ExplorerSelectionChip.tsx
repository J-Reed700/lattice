import { TextSelect, X } from 'lucide-react';

import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore } from '@/stores/explorerStore';

import { baseName, describeLines } from './describeLines';

/**
 * The lines picked in the Explorer, shown above the composer of that folder's
 * thread. They go out with the next turn; removing the chip clears them.
 */
export function ExplorerSelectionChip() {
  const explorerRoot = useConversationsStore((state) =>
    state.conversations.find((conversation) => conversation.id === state.activeConversationId)?.explorerRoot ?? null);
  const root = useExplorerStore((state) => state.root?.root ?? null);
  const openPath = useExplorerStore((state) => state.openPath);
  const selection = useExplorerStore((state) => state.selection);
  const setSelection = useExplorerStore((state) => state.setSelection);

  if (!explorerRoot || explorerRoot !== root || !openPath || !selection) return null;
  const label = `${describeLines(selection)} · ${baseName(openPath)}`;

  return (
    <div className="flex flex-wrap items-center gap-1.5 px-4 pt-3" aria-label="Lines sent with this message">
      <span className="inline-flex h-6 min-w-0 max-w-full items-center gap-1 rounded-md bg-[hsl(var(--accent)/0.1)] pl-1.5 pr-1 text-xs text-[hsl(var(--text-secondary))]">
        <TextSelect className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} aria-hidden="true" />
        <span className="truncate tabular-nums" title={`${openPath}, ${describeLines(selection)}`}>{label}</span>
        <button
          type="button"
          onClick={() => setSelection(null)}
          aria-label="Stop sending these lines"
          className="pressable inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.08)] hover:text-[hsl(var(--text-primary))]"
        >
          <X className="h-3 w-3" strokeWidth={2} />
        </button>
      </span>
    </div>
  );
}
