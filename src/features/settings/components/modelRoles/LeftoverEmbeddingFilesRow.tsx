/**
 * LeftoverEmbeddingFilesRow — files the previous (safetensors) embedding
 * runtime left in an embedding model folder. Embeddings now run from a GGUF
 * in the bundled llama.cpp, which never reads them. The app never deletes
 * them on its own: this row shows their size and removes exactly them on
 * request. The GGUF and its tokenizer stay.
 */

import { useState } from 'react';

import { FolderX } from 'lucide-react';

import { ConfirmDialog } from '@/components/ConfirmDialog';
import { useLeftoverEmbeddingFiles } from '@/features/model/hooks/useLeftoverEmbeddingFiles';
import { formatBytes } from '@/utils/formatters';

import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';

/** The folders the leftovers sit in, each once. */
function foldersOf(paths: string[]): string[] {
  return [...new Set(paths.map((path) => path.replace(/[\\/][^\\/]*$/, '')))];
}

export function LeftoverEmbeddingFilesRow() {
  const [confirming, setConfirming] = useState(false);
  const { leftovers: data, remove } = useLeftoverEmbeddingFiles();

  if (!data || data.paths.length === 0) return null;
  const folders = foldersOf(data.paths);

  return (
    <>
      <div className="flex flex-wrap items-center gap-3 border-b border-border-subtle py-4">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-text-muted">
          <FolderX className="h-[18px] w-[18px]" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-text-primary">Old embedding model files</div>
          {folders.map((folder) => (
            <div key={folder} className="truncate font-mono text-xs text-text-muted" title={folder}>
              {folder}
            </div>
          ))}
          <div className="mt-1.5 text-xs leading-relaxed tabular-nums text-text-muted">
            {formatBytes(data.total_bytes)} · left by the previous embedding runtime, no longer used
          </div>
        </div>
        <button
          type="button"
          className={SECONDARY_BUTTON_CLASS}
          onClick={() => setConfirming(true)}
          disabled={remove.isPending}
        >
          {remove.isPending ? 'Removing…' : 'Remove'}
        </button>
      </div>
      <ConfirmDialog
        isOpen={confirming}
        title="Remove old embedding files?"
        message={`Delete ${data.paths.length} old ${data.paths.length === 1 ? 'file' : 'files'} (${formatBytes(data.total_bytes)})? The current embedding model and its tokenizer stay. This cannot be undone.`}
        confirmLabel="Remove"
        variant="danger"
        onConfirm={() => {
          setConfirming(false);
          remove.mutate();
        }}
        onCancel={() => setConfirming(false)}
      />
    </>
  );
}
