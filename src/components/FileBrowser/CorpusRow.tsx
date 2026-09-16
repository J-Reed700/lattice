import { useState } from 'react';

import { Edit3, MoreHorizontal, Trash2 } from 'lucide-react';

import { metaLine } from './docMeta';
import { FileIcon } from './FileIcon';
import { type DocumentMetadata } from '../../types/fileBrowser';

export const ROW_HEIGHT = 56;

interface CorpusRowProps {
  doc: DocumentMetadata;
  isSelected: boolean;
  selectionActive: boolean;
  onClick: (_event: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onToggleSelect: () => void;
  onContextMenu: (_event: React.MouseEvent) => void;
  onRename?: () => void;
  onDelete?: () => void;
  /** Extra left padding, in px, used by the tree view for depth. */
  indent?: number;
}

/**
 * One document, one hairline row. No card, no badge: the file's own name and a
 * muted meta line carry everything. The checkbox occupies its column always and
 * only becomes visible on hover or when something is selected.
 */
export function CorpusRow({
  doc,
  isSelected,
  selectionActive,
  onClick,
  onDoubleClick,
  onToggleSelect,
  onContextMenu,
  onRename,
  onDelete,
  indent = 0,
}: CorpusRowProps) {
  const [isHovering, setIsHovering] = useState(false);
  const showCheckbox = selectionActive || isHovering || isSelected;

  return (
    <div
      role="button"
      tabIndex={0}
      aria-selected={isSelected}
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu(event);
      }}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (event.key === 'Enter') {
          event.preventDefault();
          onDoubleClick();
        } else if (event.key === ' ') {
          event.preventDefault();
          onToggleSelect();
        }
      }}
      className={`group flex cursor-pointer items-center gap-3 border-b border-border-subtle pr-3 transition-colors duration-fast ${
        isSelected ? 'bg-surface-raised' : 'hover:bg-surface'
      }`}
      style={{ height: ROW_HEIGHT, paddingLeft: 12 + indent }}
    >
      <span
        role="checkbox"
        aria-checked={isSelected}
        aria-label={`Select ${doc.fileName}`}
        tabIndex={-1}
        onClick={(event) => {
          event.stopPropagation();
          onToggleSelect();
        }}
        className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] border transition-opacity duration-fast ${
          showCheckbox ? 'opacity-100' : 'opacity-0'
        } ${
          isSelected ? 'border-accent bg-accent' : 'border-border-default bg-transparent'
        }`}
      >
        {isSelected ? (
          <svg viewBox="0 0 12 12" className="h-2.5 w-2.5 text-accent-fg" aria-hidden="true">
            <path
              d="M2 6.5L5 9.5L10 3.5"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        ) : null}
      </span>

      <FileIcon file={doc} size={16} className="shrink-0" />

      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm text-text-primary">{doc.fileName}</span>
        <span className="truncate text-xs tabular-nums text-text-muted">{metaLine(doc)}</span>
      </div>

      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-fast group-hover:opacity-100 group-focus-within:opacity-100">
        {onRename ? (
          <RowAction label="Rename" onClick={onRename}>
            <Edit3 className="h-3.5 w-3.5" strokeWidth={1.75} />
          </RowAction>
        ) : null}
        {onDelete ? (
          <RowAction label="Delete" danger onClick={onDelete}>
            <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
          </RowAction>
        ) : null}
      </div>
      <button
        type="button"
        aria-label={`Actions for ${doc.fileName}`}
        onClick={event => { event.stopPropagation(); onContextMenu(event); }}
        onDoubleClick={event => event.stopPropagation()}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-text-muted hover:bg-surface-raised hover:text-text-primary"
      >
        <MoreHorizontal className="h-4 w-4" />
      </button>
    </div>
  );
}

function RowAction({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className={`flex h-7 w-7 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast ${
        danger
          ? 'hover:bg-[hsl(var(--danger-muted))] hover:text-[hsl(var(--danger-fg))]'
          : 'hover:bg-surface-raised hover:text-text-primary'
      }`}
    >
      {children}
    </button>
  );
}
