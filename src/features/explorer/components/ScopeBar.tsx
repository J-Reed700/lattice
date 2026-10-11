import { Folder, PanelLeft, SlidersHorizontal, X } from 'lucide-react';

import type { ExplorerRoot, FolderIndexStatus } from '@/features/explorer/stores/explorerStore';

import { IndexStatusPill } from './IndexStatusPill';

export interface ScopeBarProps {
  root: ExplorerRoot;
  onClose: () => void;
  /**
   * True while closing: the index waits for its batch in flight and saves
   * before it lets go, which can take a moment on a slow model.
   */
  closing?: boolean;
  /** The folder's search index; no pill until the backend has said. */
  indexStatus?: FolderIndexStatus | null;
  onRebuildIndex?: () => void;
  onRetryIndex?: () => void;
  /** Opens the folder's settings: its space and system prompt. */
  onSettings?: () => void;
  /** Whether the folder tree is hidden; with `onToggleTree`, the bar offers the switch. */
  treeHidden?: boolean;
  onToggleTree?: () => void;
}

/**
 * The folder the Explorer and its chat are locked to. It names the folder,
 * says where its search index stands, opens the folder's settings, and
 * offers one way out: closing it,
 * back to the start screen. There is no breadcrumb or "set as scope"; a
 * different folder is a deliberate new choice.
 */
export function ScopeBar({ root, onClose, closing = false, indexStatus, onRebuildIndex, onRetryIndex, onSettings, treeHidden = false, onToggleTree }: ScopeBarProps) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle bg-chrome px-3">
      {onToggleTree && (
        <button
          type="button"
          onClick={onToggleTree}
          aria-label={treeHidden ? 'Show folder tree' : 'Hide folder tree'}
          aria-pressed={treeHidden}
          title={`${treeHidden ? 'Show' : 'Hide'} folder tree (⌘\\)`}
          className="-ml-1.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] hover:text-text-primary"
        >
          <PanelLeft className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
      <Folder className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />
      <p className="flex min-w-0 items-baseline gap-2 text-[13px]" title={root.root}>
        <span className="shrink-0 font-medium text-text-primary">{root.name}</span>
        <span className="min-w-0 truncate text-[12px] text-text-muted">{root.root}</span>
      </p>
      {indexStatus && (
        <IndexStatusPill status={indexStatus} onRebuild={() => onRebuildIndex?.()} onRetry={() => onRetryIndex?.()} />
      )}
      <span className="flex-1" aria-hidden="true" />
      {onSettings && (
        <button
          type="button"
          onClick={onSettings}
          aria-label="Folder settings"
          title="Folder settings: space and system prompt"
          className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-border-subtle text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] hover:text-text-primary"
        >
          <SlidersHorizontal className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
      <button
        type="button"
        onClick={onClose}
        disabled={closing}
        aria-busy={closing || undefined}
        className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border-subtle px-2.5 text-[12.5px] text-text-primary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] disabled:cursor-default disabled:opacity-60 disabled:hover:bg-transparent"
      >
        <X className="h-3.5 w-3.5 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
        {closing ? 'Closing…' : 'Close folder'}
      </button>
    </div>
  );
}
