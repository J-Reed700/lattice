import { Folder, X } from 'lucide-react';

import type { ExplorerRoot, FolderIndexStatus } from '@/stores/explorerStore';

import { IndexStatusPill } from './IndexStatusPill';

export interface ScopeBarProps {
  root: ExplorerRoot;
  onClose: () => void;
  /** The folder's search index; no pill until the backend has said. */
  indexStatus?: FolderIndexStatus | null;
  onRebuildIndex?: () => void;
  onRetryIndex?: () => void;
}

/**
 * The folder the Explorer and its chat are locked to. It names the folder,
 * says where its search index stands, and offers one way out: closing it,
 * back to the start screen. There is no breadcrumb or "set as scope"; a
 * different folder is a deliberate new choice.
 */
export function ScopeBar({ root, onClose, indexStatus, onRebuildIndex, onRetryIndex }: ScopeBarProps) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle bg-chrome px-3">
      <Folder className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />
      <p className="flex min-w-0 items-baseline gap-2 text-[13px]" title={root.root}>
        <span className="shrink-0 font-medium text-text-primary">{root.name}</span>
        <span className="min-w-0 truncate text-[12px] text-text-muted">{root.root}</span>
      </p>
      {indexStatus && (
        <IndexStatusPill status={indexStatus} onRebuild={() => onRebuildIndex?.()} onRetry={() => onRetryIndex?.()} />
      )}
      <span className="flex-1" aria-hidden="true" />
      <button
        type="button"
        onClick={onClose}
        className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border-subtle px-2.5 text-[12.5px] text-text-primary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)]"
      >
        <X className="h-3.5 w-3.5 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
        Close folder
      </button>
    </div>
  );
}
