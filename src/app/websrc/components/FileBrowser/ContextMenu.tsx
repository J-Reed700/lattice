/**
 * File Context Menu Component
 *
 * Purpose: Provide right-click context menu for file operations
 *
 * Actions:
 * - Open file
 * - Open in system viewer
 * - Show in folder
 * - Copy path
 * - Delete file
 * - Get file info
 */

import { useEffect, useRef, useState } from 'react';

import {
  FolderOpen,
  Copy,
  Trash2,
  Info,
  ExternalLink,
  Eye,
  Edit3,
} from 'lucide-react';

import VaultAPI from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { isSupportedFileType } from '../../utils/fileTypeDetector';

import type { ConversationSpaceDto } from '../../types';

interface ContextMenuProps {
  doc: DocumentMetadata;
  position: { x: number; y: number };
  onClose: () => void;
  onViewInRecall?: (doc: DocumentMetadata) => void;
  onRename?: (doc: DocumentMetadata) => void;
  onDelete?: (doc: DocumentMetadata) => void;
}

export function ContextMenu({ doc, position, onClose, onViewInRecall, onRename, onDelete }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [spaces, setSpaces] = useState<ConversationSpaceDto[]>([]);
  const [assignedSpaceIds, setAssignedSpaceIds] = useState<Set<string>>(new Set());
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(true);
  const [savingSpaceId, setSavingSpaceId] = useState<string | null>(null);
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

  useEffect(() => {
    let cancelled = false;

    const loadSpaceScope = async () => {
      setIsLoadingSpaces(true);

      const [spacesResult, membershipsResult] = await Promise.all([
        VaultAPI.listConversationSpaces(),
        VaultAPI.listDocumentSpaceMemberships(doc.id),
      ]);

      if (cancelled) return;

      if (spacesResult.ok) {
        setSpaces(
          spacesResult.data
            .slice()
            .sort((a, b) => Number(a.isArchived) - Number(b.isArchived) || a.name.localeCompare(b.name))
        );
      } else {
        setSpaces([]);
      }

      if (membershipsResult.ok) {
        setAssignedSpaceIds(new Set(membershipsResult.data.map((membership) => membership.spaceId)));
      } else {
        setAssignedSpaceIds(new Set());
      }

      setIsLoadingSpaces(false);
    };

    void loadSpaceScope();

    return () => {
      cancelled = true;
    };
  }, [doc.id]);

  // Adjust position to keep menu on screen
  useEffect(() => {
    if (menuRef.current) {
      const rect = menuRef.current.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;

      let adjustedX = position.x;
      let adjustedY = position.y;

      if (position.x + rect.width > viewportWidth) {
        adjustedX = viewportWidth - rect.width - 8;
      }

      if (position.y + rect.height > viewportHeight) {
        adjustedY = viewportHeight - rect.height - 8;
      }

      menuRef.current.style.left = `${adjustedX}px`;
      menuRef.current.style.top = `${adjustedY}px`;
    }
  }, [position]);

  const toggleSpaceAssignment = async (spaceId: string) => {
    const currentlyAssigned = assignedSpaceIds.has(spaceId);
    setSavingSpaceId(spaceId);

    const result = await VaultAPI.setDocumentSpaceMembership(doc.id, spaceId, !currentlyAssigned);
    if (!result.ok) {
      toast.error('Failed to update document scope', { message: result.error });
      setSavingSpaceId(null);
      return;
    }

    setAssignedSpaceIds((prev) => {
      const next = new Set(prev);
      if (currentlyAssigned) {
        next.delete(spaceId);
      } else {
        next.add(spaceId);
      }
      return next;
    });

    setSavingSpaceId(null);
  };

  const canPreview = isSupportedFileType(doc.filePath) || isWebDocument;

  const actions = [
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
      id: 'open-external',
      label: 'Open in System Viewer',
      icon: <ExternalLink className="w-4 h-4" />,
      action: async () => {
        const result = await VaultAPI.openFileById(doc.id);
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
      label: 'Show in Folder',
      icon: <FolderOpen className="w-4 h-4" />,
      action: async () => {
        const pathResult = await VaultAPI.getFilePathById(doc.id);
        if (!pathResult.ok) {
          toast.error('Failed to locate file path', { message: pathResult.error });
          onClose();
          return;
        }
        const result = await VaultAPI.showInFolder(pathResult.data);
        if (!result.ok) {
          toast.error('Failed to show in folder', { message: result.error });
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
      label: 'Copy Path',
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
      id: 'file-info',
      label: 'Get Info',
      icon: <Info className="w-4 h-4" />,
      action: () => {
        onClose();
      },
      disabled: false,
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
      className="fixed z-50 min-w-[200px] bg-[hsl(var(--surface-raised))] rounded-lg shadow-md border border-[hsl(var(--border-subtle))] py-1"
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
              className="h-px bg-[hsl(var(--surface-raised))] my-1"
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
            className={`w-full flex items-center gap-3 px-4 py-2 text-sm text-left transition-colors duration-fast ${
              isDisabled
                ? 'opacity-50 cursor-not-allowed'
                : action.dangerous
                ? 'text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))]'
                : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))]'
            }`}
            role="menuitem"
          >
            {action.icon && <span className="flex-shrink-0">{action.icon}</span>}
            <span>{action.label}</span>
          </button>
        );
      })}

      <div className="mx-2 my-1 h-px bg-[hsl(var(--surface-raised))]" />
      <div className="px-4 py-2">
        <p className="text-[11px] font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]">
          Space Scope
        </p>
      </div>

      {isLoadingSpaces && (
        <div className="px-4 pb-2 text-xs text-[hsl(var(--text-tertiary))]">Loading spaces...</div>
      )}

      {!isLoadingSpaces && spaces.length === 0 && (
        <div className="px-4 pb-2 text-xs text-[hsl(var(--text-tertiary))]">
          No spaces available.
        </div>
      )}

      {!isLoadingSpaces && spaces.map((space) => {
        const assigned = assignedSpaceIds.has(space.id);
        const isSaving = savingSpaceId === space.id;
        return (
          <button
            key={`space-${space.id}`}
            onClick={() => void toggleSpaceAssignment(space.id)}
            disabled={isSaving}
            className={`w-full flex items-center justify-between gap-3 px-4 py-2 text-sm text-left transition-colors duration-fast ${
              isSaving
                ? 'opacity-60 cursor-not-allowed text-[hsl(var(--text-tertiary))]'
                : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))]'
            }`}
            role="menuitemcheckbox"
            aria-checked={assigned}
          >
            <span className="truncate">
              {space.name}
              {space.isArchived ? ' (archived)' : ''}
            </span>
            {assigned && <span className="text-[hsl(var(--accent))]">✓</span>}
          </button>
        );
      })}
    </div>
  );
}
