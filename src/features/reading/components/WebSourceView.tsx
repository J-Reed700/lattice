import type { ReactNode } from 'react';

import { Download, ExternalLink, Loader2 } from 'lucide-react';

import { WebArticleView } from '@/features/reading/components/WebArticleView';
import { ReaderPageToggle, WebPageSnapshotView, type useWebPageMode } from '@/features/reading/snapshot';
import type { SourceWithMetadata } from '@/types/conversation';

interface WebSourceViewProps {
  source: SourceWithMetadata;
  /** The page's address, when the citation has one to open. */
  url: string | null;
  ownerKey?: string;
  citationContent?: string;
  occurrence: number | null;
  onActivePassageChange: (_text: string | null) => void;
  /** The compact reader forms leave the article's own action row out. */
  compact: boolean;
  importableUrl: string | null;
  importScopeSelect: ReactNode;
  isImportingUrl: boolean;
  onImportUrl: () => void;
  onOpenUrl: () => void;
  /** Set only when a frozen copy of the page can exist for this conversation. */
  snapshotConversationId: string | null;
  snapshotUrl: string | null;
  webPageMode: ReturnType<typeof useWebPageMode>;
}

/**
 * A web citation: the article read from its saved text, and — when the
 * conversation may hold a frozen copy of the page — the toggle between that
 * reader and the page itself.
 */
export const WebSourceView = ({
  source,
  url,
  ownerKey,
  citationContent,
  occurrence,
  onActivePassageChange,
  compact,
  importableUrl,
  importScopeSelect,
  isImportingUrl,
  onImportUrl,
  onOpenUrl,
  snapshotConversationId,
  snapshotUrl,
  webPageMode,
}: WebSourceViewProps) => {
  const reader = (
    <WebArticleView
      url={url}
      source={source}
      ownerKey={ownerKey}
      citationContent={citationContent}
      occurrence={occurrence}
      onActivePassageChange={onActivePassageChange}
      actions={
        url && !compact ? (
          <div className="inline-flex shrink-0 items-center gap-2">
            {importableUrl && importScopeSelect}
            {importableUrl && (
              <button
                type="button"
                onClick={onImportUrl}
                disabled={isImportingUrl}
                className="inline-flex items-center gap-2 rounded-md border border-border-default bg-surface-raised px-3 py-2 text-sm font-medium text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface disabled:cursor-not-allowed disabled:opacity-60"
              >
                {isImportingUrl ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Download size={14} />
                )}
                Import
              </button>
            )}
            <button
              type="button"
              onClick={onOpenUrl}
              className="inline-flex items-center gap-2 rounded-md bg-[hsl(var(--accent))] px-3 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))]"
            >
              <ExternalLink size={14} />
              Open URL
            </button>
          </div>
        ) : null
      }
    />
  );
  if (!snapshotConversationId || !snapshotUrl) return reader;
  // Until the lookup answers, the default (Page if a frozen copy exists) is unknown.
  if (webPageMode.isResolving) return <div aria-busy="true" className="h-full" />;
  const toggle = (
    <ReaderPageToggle
      mode={webPageMode.mode}
      onChange={webPageMode.setMode}
      marked={occurrence !== null && occurrence !== undefined}
    />
  );
  if (webPageMode.mode === 'page') {
    return (
      <WebPageSnapshotView
        conversationId={snapshotConversationId}
        url={snapshotUrl}
        compact={compact}
        toggle={toggle}
        onShowReader={() => webPageMode.setMode('reader')}
      />
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex shrink-0 items-center">{toggle}</div>
      <div className="min-h-0 flex-1">{reader}</div>
    </div>
  );
};
