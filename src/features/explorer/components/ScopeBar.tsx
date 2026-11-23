import { Folder, PanelLeft, SlidersHorizontal, X } from 'lucide-react';

import { Button } from '@/components/ui/button';
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
 * The folder the Explorer and its chat are locked to. Uses calm `bg-surface`
 * with hairline border to sit naturally above the workspace without surface
 * inversion in light or dark themes.
 */
export function ScopeBar({
  root,
  onClose,
  closing = false,
  indexStatus,
  onRebuildIndex,
  onRetryIndex,
  onSettings,
  treeHidden = false,
  onToggleTree,
}: ScopeBarProps) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle bg-surface px-3">
      {onToggleTree && (
        <button
          type="button"
          onClick={onToggleTree}
          aria-label={treeHidden ? 'Show folder tree' : 'Hide folder tree'}
          aria-pressed={treeHidden}
          title={`${treeHidden ? 'Show' : 'Hide'} folder tree (⌘\\)`}
          className="-ml-1 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <PanelLeft className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
      <Folder className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
      <p className="flex min-w-0 items-baseline gap-2 text-ui" title={root.root}>
        <span className="shrink-0 font-medium text-text-primary">{root.name}</span>
        <span className="min-w-0 truncate text-xs text-text-muted">{root.root}</span>
      </p>
      {indexStatus && (
        <IndexStatusPill status={indexStatus} onRebuild={() => onRebuildIndex?.()} onRetry={() => onRetryIndex?.()} />
      )}
      <span className="flex-1" aria-hidden="true" />
      {onSettings && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onSettings}
          aria-label="Folder settings"
          title="Folder settings: space and system prompt"
          className="w-7 px-0 text-text-secondary hover:text-text-primary"
        >
          <SlidersHorizontal className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        </Button>
      )}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={onClose}
        disabled={closing}
        aria-busy={closing || undefined}
      >
        <X className="h-3.5 w-3.5 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
        {closing ? 'Closing…' : 'Close folder'}
      </Button>
    </div>
  );
}
