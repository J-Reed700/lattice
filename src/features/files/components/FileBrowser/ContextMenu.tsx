/**
 * File Context Menu Component
 *
 * Purpose: Provide right-click context menu for file operations
 *
 * Actions:
 * - Open file
 * - Ask about this
 * - Open in system viewer
 * - Show in folder
 * - Rename / Copy path / Reindex
 * - Remove from index
 * - Delete file
 */

import { useEffect, useMemo, useRef } from 'react';

import {
  Check,
  FolderOpen,
  Copy,
  Trash2,
  ExternalLink,
  Eye,
  Edit3,
  EyeOff,
  MessageSquare,
  RefreshCw,
  FolderPlus,
  FolderMinus,
} from 'lucide-react';
import { useNavigate } from 'react-router';

import {
  openDocumentFile,
  showDocumentInFolder,
  useDocumentSpaceMembershipsQuery,
  useLibraryDocumentActions,
} from '@/features/files/hooks/useLibraryDocumentActions';
import { useSpacesQuery } from '@/features/spaces/api/queries';
import { toast } from '@/stores/toastStore';
import { type DocumentMetadata } from '@/types/fileBrowser';
import { isSupportedFileType } from '@/utils/fileTypeDetector';


const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

interface ContextMenuProps {
  doc: DocumentMetadata;
  position: { x: number; y: number };
  onClose: () => void;
  onViewInRecall?: (doc: DocumentMetadata) => void;
  onRename?: (doc: DocumentMetadata) => void;
  onDelete?: (doc: DocumentMetadata) => void;
  onAddToCollection?: (_doc: DocumentMetadata) => void;
  onRemoveFromCollection?: (_doc: DocumentMetadata) => void;
}

export function ContextMenu({
  doc,
  position,
  onClose,
  onViewInRecall,
  onRename,
  onDelete,
  onAddToCollection,
  onRemoveFromCollection,
}: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const spacesQuery = useSpacesQuery();
  const membershipsQuery = useDocumentSpaceMembershipsQuery(doc.id);
  const { reindex, removeFromIndex, setSpaceMembership } = useLibraryDocumentActions();
  // Open spaces first, then archived ones, each by name.
  const spaces = useMemo(
    () => (spacesQuery.data ?? [])
      .slice()
      .sort((a, b) => Number(a.isArchived) - Number(b.isArchived) || a.name.localeCompare(b.name)),
    [spacesQuery.data]
  );
  const assignedSpaceIds = useMemo(
    () => new Set((membershipsQuery.data ?? []).map((membership) => membership.spaceId)),
    [membershipsQuery.data]
  );
  const isLoadingSpaces = spacesQuery.isPending || membershipsQuery.isPending;
  const savingSpaceId = setSpaceMembership.isPending ? setSpaceMembership.variables?.spaceId ?? null : null;
  const normalizedCategory = doc.category.toLowerCase().replace(/[_-]+/g, ' ').trim();
  const isWebDocument =
    normalizedCategory.includes('web article') ||
    normalizedCategory === 'web' ||
    /^https?:\/\//i.test(doc.filePath) ||
    doc.filePath.includes('/.lattice/web-archive/');

  // Close on click outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      // Runtime type guard instead of type cast
      if (event.target instanceof Node && menuRef.current && !menuRef.current.contains(event.target)) {
        onClose();
      }
    }

    // Close on escape key
    function handleEscape(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        onClose();
      }
    }

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [onClose]);

  // Adjust position to keep menu on screen
  useEffect(() => {
    if (menuRef.current) {
      const rect = menuRef.current.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;

      let adjustedX = position.x;
      let adjustedY = position.y;

      if (position.x + rect.width > viewportWidth) {
        adjustedX = Math.max(8, viewportWidth - rect.width - 8);
      }

      if (position.y + rect.height > viewportHeight) {
        adjustedY = Math.max(8, viewportHeight - rect.height - 8);
      }

      menuRef.current.style.left = `${adjustedX}px`;
      menuRef.current.style.top = `${adjustedY}px`;
    }
  }, [position, spaces.length, isLoadingSpaces]);

  const toggleSpaceAssignment = async (spaceId: string) => {
    try {
      await setSpaceMembership.mutateAsync({ documentId: doc.id, spaceId, assigned: !assignedSpaceIds.has(spaceId) });
    } catch (error) {
      toast.error('Failed to update document scope', { message: errorMessage(error) });
    }
  };

  const canPreview = isSupportedFileType(doc.filePath) || isWebDocument;

  const actions = [
    ...(onAddToCollection ? [{
      id: 'add-to-collection',
      label: 'Add to collection…',
      icon: <FolderPlus className="w-4 h-4" />,
      action: () => { onClose(); onAddToCollection(doc); },
      disabled: false,
    }] : []),
    ...(onRemoveFromCollection ? [{
      id: 'remove-from-collection',
      label: 'Remove from collection',
      icon: <FolderMinus className="w-4 h-4" />,
      action: () => { onClose(); onRemoveFromCollection(doc); },
      disabled: false,
    }] : []),
    ...(canPreview && onViewInRecall ? [{
      id: 'view-in-lattice',
      label: 'View in Lattice',
      icon: <Eye className="w-4 h-4" />,
      action: () => {
        onViewInRecall(doc);
        onClose();
      },
      disabled: false,
    }] : []),
    {
      id: 'ask-about-this',
      label: 'Ask about this',
      icon: <MessageSquare className="w-4 h-4" />,
      action: () => {
        navigate(`/chat?${new URLSearchParams({ new: '1', documentId: doc.id }).toString()}`);
        onClose();
      },
      disabled: false,
    },
    {
      id: 'open-external',
      label: 'Open in system viewer',
      icon: <ExternalLink className="w-4 h-4" />,
      action: async () => {
        const result = await openDocumentFile(doc.id);
        if (!result.ok) {
          toast.error('Failed to open file in system viewer', {
            message: result.error,
          });
        } else if (result.data.action === 'render_internal' && onViewInRecall) {
          // Web article detected - use internal viewer
          onViewInRecall(doc);
        }
        // For opened_external, file is already open - just close menu
        onClose();
      },
      disabled: false,
    },
    {
      id: 'show-in-folder',
      label: 'Show in folder',
      icon: <FolderOpen className="w-4 h-4" />,
      action: async () => {
        try {
          await showDocumentInFolder(doc.id);
        } catch (error) {
          toast.error('Failed to show in folder', { message: errorMessage(error) });
        }
        onClose();
      },
      disabled: /^https?:\/\//i.test(doc.filePath),
    },
    {
      id: 'separator-1',
      label: '',
      separator: true,
      action: () => {},
      disabled: false,
    },
    {
      id: 'rename',
      label: 'Rename',
      icon: <Edit3 className="w-4 h-4" />,
      action: () => {
        if (onRename) {
          onRename(doc);
        }
        onClose();
      },
      disabled: !onRename,
    },
    {
      id: 'copy-path',
      label: 'Copy path',
      icon: <Copy className="w-4 h-4" />,
      action: async () => {
        try {
          await navigator.clipboard.writeText(doc.filePath);
        } catch (error) {
          console.error('Failed to copy path:', error);
        }
        onClose();
      },
      disabled: false,
    },
    {
      id: 'reindex',
      label: 'Reindex',
      icon: <RefreshCw className="w-4 h-4" />,
      action: async () => {
        try {
          await reindex.mutateAsync(doc);
          toast.success(`Reindexing ${doc.fileName}`);
        } catch (error) {
          toast.error(`Couldn't reindex ${doc.fileName}`, { message: errorMessage(error) });
        }
        onClose();
      },
      disabled: isWebDocument,
    },
    {
      id: 'separator-2',
      label: '',
      separator: true,
      action: () => {},
      disabled: false,
    },
    {
      id: 'remove-from-index',
      label: 'Remove from index',
      icon: <EyeOff className="w-4 h-4" />,
      action: async () => {
        try {
          await removeFromIndex.mutateAsync(doc);
          toast.success(`Removed ${doc.fileName} from the index`);
        } catch (error) {
          toast.error(`Couldn't remove ${doc.fileName} from the index`, { message: errorMessage(error) });
        }
        onClose();
      },
      disabled: isWebDocument,
    },
    {
      id: 'delete',
      label: 'Delete',
      icon: <Trash2 className="w-4 h-4" />,
      action: () => {
        if (onDelete) {
          onDelete(doc);
        } else {
          console.warn('[ContextMenu.delete] onDelete callback is missing!');
        }
        onClose();
      },
      dangerous: true,
      disabled: !onDelete,
    },
  ];

  return (
    <div
      ref={menuRef}
      className="fixed z-50 max-h-[calc(100vh-16px)] min-w-[200px] overflow-y-auto rounded-md border border-border-subtle bg-surface-raised py-1 shadow-md"
      style={{
        left: position.x,
        top: position.y,
      }}
      role="menu"
      aria-label="File context menu"
    >
      {actions.map((action) => {
        if (action.separator) {
          return (
            <div
              key={action.id}
              className="my-1 h-px bg-border-subtle"
              role="separator"
            />
          );
        }

        const isDisabled = action.disabled || false;

        return (
          <button
            key={action.id}
            onClick={() => !isDisabled && action.action()}
            disabled={isDisabled}
            className={`flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition-colors duration-fast ${
              isDisabled
                ? 'cursor-not-allowed opacity-50'
                : action.dangerous
                ? 'text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))]'
                : 'text-text-secondary hover:bg-surface'
            }`}
            role="menuitem"
          >
            {action.icon && <span className="shrink-0">{action.icon}</span>}
            <span>{action.label}</span>
          </button>
        );
      })}

      <div className="mx-2 my-1 h-px bg-border-subtle" />
      <div className="px-4 pb-1 pt-2">
        <p className="text-xs text-text-muted">Spaces</p>
      </div>

      {isLoadingSpaces && (
        <div className="px-4 pb-2 text-xs text-text-muted">Loading…</div>
      )}

      {!isLoadingSpaces && spaces.length === 0 && (
        <div className="px-4 pb-2 text-xs text-text-muted">No spaces yet.</div>
      )}

      {!isLoadingSpaces && spaces.map((space) => {
        const assigned = assignedSpaceIds.has(space.id);
        const isSaving = savingSpaceId === space.id;
        return (
          <button
            key={`space-${space.id}`}
            onClick={() => void toggleSpaceAssignment(space.id)}
            disabled={isSaving}
            className={`flex w-full items-center justify-between gap-3 px-4 py-2 text-left text-sm transition-colors duration-fast ${
              isSaving
                ? 'cursor-not-allowed text-text-muted opacity-60'
                : 'text-text-secondary hover:bg-surface'
            }`}
            role="menuitemcheckbox"
            aria-checked={assigned}
          >
            <span className="truncate">
              {space.name}
              {space.isArchived ? ' (archived)' : ''}
            </span>
            {assigned && <Check className="h-3.5 w-3.5 shrink-0 text-accent" strokeWidth={2} />}
          </button>
        );
      })}
    </div>
  );
}
