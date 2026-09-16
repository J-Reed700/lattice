import { X } from 'lucide-react';

import { IconButton } from '@/components/ui';

import type { StagedFile } from './useChatFileDrop';

/**
 * The staged-files row above the composer (BRIEF rank 5, contract §4.5).
 *
 * The scope line lives in the linked-documents footer instead: this row only
 * exists while files are staged, and the narrowing it used to announce outlives
 * the staging by the whole conversation.
 */

export interface ChatDropStagingProps {
  staged: StagedFile[];
  isImporting: boolean;
  onRemove: (_path: string) => void;
  onImport: () => void;
  onClear: () => void;
}

export function ChatDropStaging({
  staged,
  isImporting,
  onRemove,
  onImport,
  onClear,
}: ChatDropStagingProps) {
  if (staged.length === 0) return null;

  const preview = staged.slice(0, 3);

  return (
    <div className="border-t border-subtle px-6 py-2 text-xs">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2 text-[hsl(var(--text-secondary))]">
          <span className="shrink-0">
            {isImporting
              ? `Adding ${staged.length} file${staged.length !== 1 ? 's' : ''}…`
              : `${staged.length} file${staged.length !== 1 ? 's' : ''} ready`}
          </span>
          <span className="min-w-0 truncate text-[hsl(var(--text-muted))]">
            {preview.map((file, index) => (
              <span key={file.path}>
                {index > 0 && ' · '}
                {file.name}
                {!isImporting && (
                  <IconButton
                    label={`Remove ${file.name}`}
                    className="ml-1 h-4 w-4 align-middle [&>svg]:h-3 [&>svg]:w-3"
                    onClick={() => onRemove(file.path)}
                  >
                    <X />
                  </IconButton>
                )}
              </span>
            ))}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <button
            type="button"
            onClick={onClear}
            disabled={isImporting}
            className="text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))] disabled:cursor-not-allowed disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onImport}
            disabled={isImporting}
            className="text-[hsl(var(--accent))] underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:opacity-50"
          >
            Add to this conversation
          </button>
        </div>
      </div>
    </div>
  );
}
