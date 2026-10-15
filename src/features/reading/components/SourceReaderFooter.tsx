import type { ReactNode } from 'react';

import { Download, ExternalLink, Loader2 } from 'lucide-react';

interface SourceReaderFooterProps {
  canShowInFolder: boolean;
  onShowInFolder: () => void;
  platformRevealLabel: string;
  /** A web source that can be imported: show the scope picker and Import. */
  canImportUrl: boolean;
  importScopeSelect: ReactNode;
  isImportingUrl: boolean;
  onImportUrl: () => void;
  primaryActionLabel: string;
  onOpenPrimary: () => void;
}

/** The reader's action row: reveal the file, import a web source, open it outside. */
export const SourceReaderFooter = ({
  canShowInFolder,
  onShowInFolder,
  platformRevealLabel,
  canImportUrl,
  importScopeSelect,
  isImportingUrl,
  onImportUrl,
  primaryActionLabel,
  onOpenPrimary,
}: SourceReaderFooterProps) => (
  <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-subtle px-5 py-3">
    {canShowInFolder && (
      <button
        onClick={onShowInFolder}
        className="flex items-center gap-2 rounded-md border border-border-default bg-surface px-4 py-2 text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised"
      >
        <ExternalLink size={16} />
        <span>{platformRevealLabel}</span>
      </button>
    )}

    {canImportUrl && importScopeSelect}

    {canImportUrl && (
      <button
        onClick={onImportUrl}
        disabled={isImportingUrl}
        className="flex items-center gap-2 rounded-md border border-border-default bg-surface px-4 py-2 text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-60"
      >
        {isImportingUrl ? <Loader2 size={16} className="animate-spin" /> : <Download size={16} />}
        <span>Import</span>
      </button>
    )}

    <button
      onClick={onOpenPrimary}
      className="flex items-center gap-2 rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
    >
      <Download size={16} />
      <span>{primaryActionLabel}</span>
    </button>
  </div>
);
