import { FolderOpen, FolderTree } from 'lucide-react';

import { cn } from '@/lib/utils';

import { YourFolders } from './ExplorerFolders';
import { useExplorerFolders, type ExplorerFolder } from './useExplorerFolders';

export interface ExplorerStartProps {
  /** The root being opened, if any. */
  opening: string | null;
  /** Opens the system folder picker, starting at `defaultPath` when given. */
  onChoose: (_defaultPath?: string) => void;
  onOpen: (_folder: ExplorerFolder) => void;
}

/**
 * Before a folder is open there is nothing to browse: one large choice, and
 * the folders picked before. With none yet, the choice is the whole page;
 * once there are some, it steps down so the list is what the eye lands on.
 * Once picked, the Explorer stays in that folder until it is closed.
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
      className={cn('flex flex-1 justify-center overflow-y-auto px-4', hasFolders ? 'py-10 sm:py-14' : 'items-center py-10', loading && 'invisible')}
    >
      <div className={cn('w-full', hasFolders ? 'max-w-2xl' : 'max-w-lg')}>
        <FolderTree className="h-6 w-6 text-text-muted" strokeWidth={1.5} aria-hidden="true" />
        <h1 className="mt-4 font-serif text-2xl text-text-primary">Pick the folder to work in.</h1>
        <p className="mt-2 text-sm leading-6 text-text-secondary">
          The Explorer and its chat stay inside the folder you pick. They can read anything below it and nothing above it.
          To work somewhere else, close the folder and pick again. Nothing in the folder is changed.
        </p>
        {!hasFolders && (
          <p className="mt-2 text-sm leading-6 text-text-secondary">
            Lattice also indexes the folder so its chat can search it by meaning. The index lives in Lattice&apos;s data, not in
            the folder, and stays until you delete it.
          </p>
        )}
        <button
          type="button"
          disabled={busy}
          onClick={() => onChoose(recent)}
          className={cn(
            'pressable mt-6 flex w-full items-center rounded-lg bg-accent text-left text-accent-fg shadow-action transition hover:brightness-95 disabled:opacity-60',
            hasFolders ? 'gap-3 px-5 py-3.5' : 'gap-4 px-6 py-6'
          )}
        >
          <FolderOpen className={cn('shrink-0', hasFolders ? 'h-5 w-5' : 'h-7 w-7')} strokeWidth={1.5} aria-hidden="true" />
          <span className={cn('flex min-w-0', hasFolders ? 'flex-1 items-baseline justify-between gap-3' : 'flex-col')}>
            <span className={cn('font-medium', hasFolders ? 'text-ui' : 'text-base')}>{busy ? 'Opening…' : 'Choose a folder…'}</span>
            <span className="truncate text-[12.5px] opacity-80">Opens the system folder picker</span>
          </span>
        </button>
        <YourFolders list={list} opening={opening} onOpen={onOpen} />
        {hasFolders && (
          <p className="mt-3 px-1 text-[11.5px] leading-5 text-text-muted">
            Each folder&apos;s index lives in Lattice&apos;s data, not in the folder. Delete index frees the space; the next open
            builds it again.
          </p>
        )}
        {list.error && !hasFolders && (
          <p role="alert" className="mt-6 text-[12.5px] text-text-muted">
            Your folders couldn&apos;t be listed: {list.error}
          </p>
        )}
      </div>
    </div>
  );
}
