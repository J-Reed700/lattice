import { FolderOpen, FolderTree } from 'lucide-react';

import { Button, PageHeader } from '@/components/ui';
import { cn } from '@/lib/utils';

import { YourFolders } from './ExplorerFolders';
import { useExplorerFolders, type ExplorerFolder } from '../hooks/useExplorerFolders';

export interface ExplorerStartProps {
  /** The root being opened, if any. */
  opening: string | null;
  /** Opens the system folder picker, starting at `defaultPath` when given. */
  onChoose: (_defaultPath?: string) => void;
  onOpen: (_folder: ExplorerFolder) => void;
}

/**
 * Explorer start screen: editorial PageHeader with a calm `--action` primary
 * "Choose a folder" CTA and a structured 2-column grid of indexed folder
 * workspace cards.
 */
export function ExplorerStart({ opening, onChoose, onOpen }: ExplorerStartProps) {
  const list = useExplorerFolders();
  const hasFolders = (list.folders?.length ?? 0) > 0;
  // Held back for the moment the list takes, so the page does not jump from
  // one layout to the other.
  const loading = list.folders === null && list.error === null;
  const recent = list.folders?.find((folder) => folder.exists)?.root;
  const busy = opening !== null;

  return (
    <div
      aria-busy={loading || undefined}
      className={cn(
        'flex flex-1 justify-center overflow-y-auto bg-bg px-6',
        hasFolders ? 'py-10' : 'items-center py-10',
        loading && 'invisible'
      )}
    >
      <div className={cn('w-full', hasFolders ? 'max-w-[960px]' : 'max-w-[640px]')}>
        <PageHeader
          title="Folder Explorer"
          description="Browse and chat inside a single folder. Lattice reads only below it and never modifies files on disk."
          actions={
            <Button
              type="button"
              disabled={busy}
              onClick={() => onChoose(recent)}
            >
              <FolderOpen className="h-4 w-4" strokeWidth={1.75} aria-hidden="true" />
              {busy ? 'Opening…' : 'Choose a folder'}
            </Button>
          }
        />

        {!hasFolders && (
          <div className="rounded-xl border border-border-subtle bg-surface p-6 shadow-control">
            <div className="flex items-start gap-4">
              <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-surface-sunken text-text-secondary ring-1 ring-inset ring-border-subtle">
                <FolderTree className="h-5 w-5" strokeWidth={1.6} aria-hidden="true" />
              </span>
              <div className="min-w-0 flex-1">
                <h2 className="text-[15px] font-medium text-text-primary">
                  Open a local folder workspace
                </h2>
                <p className="mt-1.5 text-sm leading-relaxed text-text-secondary">
                  The Explorer and its chat stay inside the folder you pick. To work somewhere else,
                  close the folder and pick again.
                </p>
                <p className="mt-2 text-sm leading-relaxed text-text-secondary">
                  Lattice also indexes the folder so its chat can search it by meaning. The index lives in Lattice&apos;s data, not in
                  the folder, and stays until you delete it.
                </p>
              </div>
            </div>
          </div>
        )}

        <YourFolders list={list} opening={opening} onOpen={onOpen} />

        {hasFolders && (
          <p className="mt-4 px-0.5 text-xs leading-5 text-text-muted">
            Each folder&apos;s index lives in Lattice&apos;s data, not in the folder. Delete index frees the space; the next open
            builds it again.
          </p>
        )}

        {list.error && !hasFolders && (
          <p role="alert" className="mt-6 text-xs text-danger-fg">
            Your folders couldn&apos;t be listed: {list.error}
          </p>
        )}
      </div>
    </div>
  );
}
