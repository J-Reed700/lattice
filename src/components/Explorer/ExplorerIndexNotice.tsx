import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore, type FolderIndexStatus } from '@/stores/explorerStore';

import { count, indexPercent } from './indexProgress';

/** The line's words while the index builds; `null` once it is ready. Exported for tests. */
export function describeIndexNotice(status: FolderIndexStatus | null): string | null {
  if (status?.state === 'indexing') {
    return `Indexing this folder · ${indexPercent(status.passagesEmbedded, status.passagesTotal)}%`;
  }
  if (status?.state === 'scanning') {
    return status.filesTotal > 0 ? `Scanning this folder · ${count(status.filesTotal)} files` : 'Scanning this folder';
  }
  return null;
}

/**
 * One quiet line above the composer of a folder's thread while its index is
 * still building, so an answer that misses something is not a mystery. It
 * fades in after a beat, so the quick rescan of an indexed folder does not
 * flash it, and goes when the index is ready.
 */
export function ExplorerIndexNotice() {
  const explorerRoot = useConversationsStore((state) =>
    state.conversations.find((conversation) => conversation.id === state.activeConversationId)?.explorerRoot ?? null);
  const root = useExplorerStore((state) => state.root?.root ?? null);
  const status = useExplorerStore((state) => state.indexStatus);

  const words = describeIndexNotice(status);
  if (!explorerRoot || explorerRoot !== root || !words) return null;

  return (
    <p
      role="status"
      title={`${words} — search covers what's indexed so far`}
      className="explorer-index-notice flex items-center gap-2 border-t border-subtle px-6 py-1.5 text-xs tabular-nums text-text-muted"
    >
      <span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-[hsl(var(--accent)/0.7)]" />
      <span className="min-w-0 truncate">
        <span className="text-text-secondary">{words}</span> — search covers what&apos;s indexed so far
      </span>
    </p>
  );
}
