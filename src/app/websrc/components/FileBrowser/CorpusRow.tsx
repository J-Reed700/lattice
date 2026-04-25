import { useState } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { Edit3, MessageCircle, MoreHorizontal, Trash2 } from 'lucide-react';

import { FileIcon } from './FileIcon';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { formatRelativeTime } from '../../utils/dateUtils';

interface CorpusRowProps {
  doc: DocumentMetadata;
  density: 'list' | 'detail';
  isSelected: boolean;
  selectionActive: boolean;
  onToggleSelect: (withModifier: 'toggle' | 'range' | 'single') => void;
  onOpen: () => void;
  onChatWith: () => void;
  onRename: () => void;
  onDelete: () => void;
  onContextMenu: (event: React.MouseEvent) => void;
}

const LIST_HEIGHT = 44;
const DETAIL_HEIGHT = 64;

export const ROW_HEIGHTS = { list: LIST_HEIGHT, detail: DETAIL_HEIGHT } as const;

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);

const categoryChip = (doc: DocumentMetadata): string => {
  if (isHttpUrl(doc.filePath) || doc.filePath.includes('/.recall/web-archive/')) return 'WEB';
  const normalizedCategory = normalizeCategory(doc.category);
  if (normalizedCategory.includes('pdf')) return 'PDF';
  const fileType = (doc.fileType ?? '').toLowerCase();
  if (fileType === 'pdf') return 'PDF';
  if (['md', 'markdown', 'txt', 'rtf'].includes(fileType)) return 'NOTE';
  if (['doc', 'docx'].includes(fileType)) return 'DOC';
  if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'html', 'css', 'json', 'xml', 'yml', 'yaml'].includes(fileType)) return 'CODE';
  if (['mp3', 'wav', 'flac', 'ogg', 'm4a', 'mp4', 'mkv', 'mov', 'avi', 'webm'].includes(fileType)) return 'MEDIA';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(fileType)) return 'IMG';
  if (fileType) return fileType.toUpperCase();
  return 'FILE';
};

const ancestorPath = (filePath: string): string => {
  if (isHttpUrl(filePath)) return 'Web';
  const normalized = filePath.replace(/\\/g, '/');
  const segments = normalized.split('/').filter(Boolean);
  if (segments.length <= 1) return '';
  return segments[segments.length - 2];
};

/**
 * Corpus row — §6.2 (List 44px / Detail 64px) + §15.8 "Chat with this"
 * as the first hover action in the right zone.
 */
export function CorpusRow({
  doc,
  density,
  isSelected,
  selectionActive,
  onToggleSelect,
  onOpen,
  onChatWith,
  onRename,
  onDelete,
  onContextMenu,
}: CorpusRowProps) {
  const [isHovering, setIsHovering] = useState(false);
  const prefersReducedMotion = useReducedMotion();

  const rowHeight = density === 'detail' ? DETAIL_HEIGHT : LIST_HEIGHT;
  const category = categoryChip(doc);
  const showCheckbox = selectionActive || isHovering || isSelected;
  const showHoverActions = isHovering;
  const ancestor = ancestorPath(doc.filePath);

  const handleClick = (event: React.MouseEvent) => {
    if ((event.target as HTMLElement).closest('[data-row-action]')) {
      return;
    }
    if (event.metaKey || event.ctrlKey) {
      onToggleSelect('toggle');
      return;
    }
    if (event.shiftKey) {
      onToggleSelect('range');
      return;
    }
    onOpen();
  };

  const handleCheckboxClick = (event: React.MouseEvent) => {
    event.stopPropagation();
    onToggleSelect('toggle');
  };

  const handleContextMenu = (event: React.MouseEvent) => {
    onContextMenu(event);
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      onOpen();
    } else if (event.key === ' ') {
      event.preventDefault();
      onToggleSelect('toggle');
    }
  };

  return (
    <div
      role="button"
      tabIndex={0}
      aria-selected={isSelected}
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      onContextMenu={handleContextMenu}
      className={`group relative flex cursor-pointer items-center gap-3 px-4 transition-colors duration-fast ${
        isSelected
          ? 'bg-[hsl(var(--accent-muted))]'
          : 'hover:bg-[hsl(var(--surface))]'
      }`}
      style={{ height: rowHeight }}
    >
      {isSelected &&
        (prefersReducedMotion ? (
          <span
            aria-hidden="true"
            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
          />
        ) : (
          <motion.span
            layoutId="files-active-bar"
            aria-hidden="true"
            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        ))}

      <div
        className={`flex h-3 w-3 shrink-0 items-center justify-center rounded-[3px] border transition-opacity duration-fast ${
          showCheckbox ? 'opacity-100' : 'opacity-0'
        } ${
          isSelected
            ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent))]'
            : 'border-[hsl(var(--border-default))] bg-transparent'
        }`}
        data-row-action="checkbox"
        onClick={handleCheckboxClick}
        role="checkbox"
        aria-checked={isSelected}
        aria-label={isSelected ? 'Deselect document' : 'Select document'}
      >
        {isSelected && (
          <svg
            viewBox="0 0 12 12"
            className="h-2 w-2 text-[hsl(var(--accent-fg))]"
            aria-hidden="true"
          >
            <path
              d="M2 6.5L5 9.5L10 3.5"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        )}
      </div>

      <FileIcon
        file={doc}
        size={14}
        className="shrink-0 text-[hsl(var(--text-tertiary))]"
      />

      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-baseline gap-2">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-[hsl(var(--text-primary))]">
            {doc.fileName}
          </span>
          <span className="shrink-0 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-tertiary))]">
            {category}
          </span>
        </div>
        {density === 'detail' && (
          <p className="truncate text-xs text-[hsl(var(--text-tertiary))]">
            {[
              doc.category || null,
              (doc.fileType || '').toUpperCase() || null,
              doc.wordCount > 0 ? `${doc.wordCount.toLocaleString()} words` : null,
              ancestor || null,
            ]
              .filter(Boolean)
              .join(' · ')}
          </p>
        )}
      </div>

      {!showHoverActions && (
        <time className="shrink-0 text-xs tabular-nums text-[hsl(var(--text-muted))]">
          {formatRelativeTime(doc.modifiedAt)}
        </time>
      )}

      {showHoverActions && (
        <div
          className="absolute right-3 flex items-center gap-0.5"
          data-row-action="hover"
          onClick={(event) => event.stopPropagation()}
        >
          <IconActionButton
            label="Start a chat about this document"
            onClick={(event) => {
              event.stopPropagation();
              onChatWith();
            }}
          >
            <MessageCircle className="h-3.5 w-3.5" strokeWidth={1.75} />
          </IconActionButton>
          <IconActionButton
            label="Rename"
            onClick={(event) => {
              event.stopPropagation();
              onRename();
            }}
          >
            <Edit3 className="h-3.5 w-3.5" strokeWidth={1.75} />
          </IconActionButton>
          <IconActionButton
            label="Delete"
            danger
            onClick={(event) => {
              event.stopPropagation();
              onDelete();
            }}
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
          </IconActionButton>
          <IconActionButton
            label="More actions"
            onClick={(event) => {
              event.stopPropagation();
              onContextMenu(event);
            }}
          >
            <MoreHorizontal className="h-3.5 w-3.5" strokeWidth={1.75} />
          </IconActionButton>
        </div>
      )}
    </div>
  );
}

function IconActionButton({
  label,
  onClick,
  children,
  danger,
}: {
  label: string;
  onClick: (event: React.MouseEvent) => void;
  children: React.ReactNode;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-6 w-6 items-center justify-center rounded-[var(--radius-sm)] text-[hsl(var(--text-tertiary))] transition-colors duration-fast ${
        danger
          ? 'hover:bg-[hsl(var(--danger-muted))] hover:text-[hsl(var(--danger-fg))]'
          : 'hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
      }`}
    >
      {children}
    </button>
  );
}
