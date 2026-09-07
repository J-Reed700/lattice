import { PanelRight } from 'lucide-react';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { useNeighborhoodQuery } from './useNeighborhoodQuery';
import { IconButton } from '../ui/IconButton';


interface NeighborhoodPanelProps {
  documentId: string;
  /** The document's file name, for the backlink lookup. */
  documentTitle: string;
  documentsById: Map<string, DocumentMetadata>;
  onOpenDocument: (_documentId: string) => void;
  onOpenConversation: (_conversationId: string) => void;
  onClose: () => void;
}

interface NeighborhoodListItem {
  id: string;
  title: string;
}

interface NeighborhoodListProps {
  heading: string;
  items: NeighborhoodListItem[];
  emptyLabel: string;
  isLoading: boolean;
  onSelect: (_id: string) => void;
}

function NeighborhoodList({ heading, items, emptyLabel, isLoading, onSelect }: NeighborhoodListProps) {
  return (
    <section>
      <h4 className="pb-1 text-xs font-medium text-text-secondary">{heading}</h4>
      <div className="border-t border-border-subtle">
        {isLoading ? (
          <p className="border-b border-border-subtle py-2 text-xs text-text-muted">Loading…</p>
        ) : items.length === 0 ? (
          <p className="border-b border-border-subtle py-2 text-xs text-text-muted">{emptyLabel}</p>
        ) : (
          items.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelect(item.id)}
              className="flex w-full items-center border-b border-border-subtle py-2 text-left text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              <span className="truncate">{item.title}</span>
            </button>
          ))
        )}
      </div>
    </section>
  );
}

/**
 * What sits next to the focused document.
 *
 * Three separate lists, titles only. "Similar" is labelled and never scored —
 * a percentage next to a guess reads as a fact — and it is never mixed into the
 * authored links above it.
 */
export function NeighborhoodPanel({
  documentId,
  documentTitle,
  documentsById,
  onOpenDocument,
  onOpenConversation,
  onClose,
}: NeighborhoodPanelProps) {
  const query = useNeighborhoodQuery(documentId, documentTitle, documentsById);
  const isLoading = query.isLoading;
  const data = query.data;

  return (
    <aside className="flex w-[260px] shrink-0 flex-col gap-6 overflow-y-auto border-l border-border-subtle py-4 pl-4">
      <div className="flex h-7 items-center justify-between">
        <h3 className="truncate text-xs font-medium text-text-secondary">Related</h3>
        <IconButton label="Hide related" tooltipSide="left" onClick={onClose}>
          <PanelRight strokeWidth={1.75} />
        </IconButton>
      </div>

      <NeighborhoodList
        heading="Links here"
        items={(data?.links ?? []).map((link) => ({ id: link.documentId, title: link.title }))}
        emptyLabel="Nothing links here yet."
        isLoading={isLoading}
        onSelect={onOpenDocument}
      />
      <NeighborhoodList
        heading="Similar"
        items={(data?.similar ?? []).map((hit) => ({ id: hit.documentId, title: hit.title }))}
        emptyLabel="Nothing similar yet."
        isLoading={isLoading}
        onSelect={onOpenDocument}
      />
      <NeighborhoodList
        heading="Cited in"
        items={(data?.citedIn ?? []).map((row) => ({ id: row.conversationId, title: row.title }))}
        emptyLabel="Not cited in a conversation yet."
        isLoading={isLoading}
        onSelect={onOpenConversation}
      />
    </aside>
  );
}
