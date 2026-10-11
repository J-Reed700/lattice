import { useCallback, useMemo, useState } from 'react';

import {
  useExplorerFolderMutations,
  useExplorerFoldersQuery,
  useExplorerIndexStatuses,
} from '@/features/explorer/api/queries';
import type { FolderIndexStatus } from '@/features/explorer/stores/explorerStore';
import { toast } from '@/stores/toastStore';

/** What a folder's index holds, as the backend's `FolderIndexSummaryState`. */
export type FolderIndexSummaryState = 'indexed' | 'partial' | 'indexing' | 'notIndexed' | 'tooLarge' | 'refused' | 'error';

/** As the backend's `FolderIndexSummaryDto`: live for the open folder, from disk for the rest. */
export interface FolderIndexSummary {
  state: FolderIndexSummaryState;
  filesTotal: number;
  filesIndexed: number;
  passagesTotal: number;
  passagesEmbedded: number;
  /** Bytes of the folder's own index; 0 when it has none. */
  bytes: number;
  /** The enclosing folder whose index this one reuses. */
  indexRoot: string | null;
  etaSeconds: number | null;
  message: string | null;
}

/** One row of "Your folders", as the backend's `ExplorerFolderDto`. */
export interface ExplorerFolder {
  root: string;
  name: string;
  pinned: boolean;
  addedAt: string;
  lastOpenedAt: string;
  /** Still on disk. */
  exists: boolean;
  threadCount: number;
  index: FolderIndexSummary;
  /** The folder's system prompt; `null` when the space's applies. */
  instructions: string | null;
  /** The space its threads belong to; General unless one was chosen. */
  spaceId: string;
  /** The thread the folder's chat last showed; `null` when none or deleted. */
  lastThreadId: string | null;
}

/** A live status event as the list shows it; `unavailable` keeps what the list had. */
export function summaryFromStatus(status: FolderIndexStatus, previous: FolderIndexSummary): FolderIndexSummary {
  const states: Record<FolderIndexStatus['state'], FolderIndexSummaryState> = {
    scanning: 'indexing',
    indexing: 'indexing',
    ready: 'indexed',
    tooLarge: 'tooLarge',
    refused: 'refused',
    unavailable: previous.state === 'indexing' ? 'partial' : previous.state,
    error: 'error',
  };
  return {
    ...previous,
    state: states[status.state],
    filesTotal: status.filesTotal,
    filesIndexed: status.filesIndexed,
    passagesTotal: status.passagesTotal,
    passagesEmbedded: status.passagesEmbedded,
    etaSeconds: status.etaSeconds,
    message: status.message,
    indexRoot: status.indexRoot !== status.root ? status.indexRoot : null,
  };
}

export interface ExplorerFolders {
  /** `null` until the first load lands. */
  folders: ExplorerFolder[] | null;
  home: string | null;
  error: string | null;
  /** Roots with an action in flight. */
  busy: ReadonlySet<string>;
  reload: () => Promise<void>;
  rename: (_root: string, _name: string) => Promise<void>;
  setPinned: (_root: string, _pinned: boolean) => Promise<void>;
  deleteIndex: (_root: string) => Promise<void>;
  /** Resolves `true` when removed; a failure is toasted and resolves `false`. */
  remove: (_root: string, _deleteThreads: boolean) => Promise<boolean>;
  /** Resolves `true` when saved; a failure is toasted and resolves `false`. */
  saveSettings: (_root: string, _instructions: string, _spaceId: string) => Promise<boolean>;
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * The folders list, kept fresh by its actions and by index status events, so
 * a row that is indexing moves live.
 */
export function useExplorerFolders(): ExplorerFolders {
  const query = useExplorerFoldersQuery();
  const mutations = useExplorerFolderMutations();
  const [busy, setBusy] = useState<ReadonlySet<string>>(() => new Set());
  const list = query.data;
  const roots = useMemo(() => list?.folders.map((folder) => folder.root) ?? [], [list]);
  // A status from before the list was read is already in it.
  const live = useExplorerIndexStatuses(roots, query.dataUpdatedAt);

  const shown = useMemo(
    () =>
      list?.folders.map((folder) => {
        const status = live.get(folder.root);
        return status ? { ...folder, index: summaryFromStatus(status, folder.index) } : folder;
      }) ?? null,
    [list, live]
  );

  /** Runs one row's action with the row marked busy. */
  const withBusy = useCallback(async <T,>(root: string, action: () => Promise<T>): Promise<T> => {
    setBusy((current) => new Set(current).add(root));
    try {
      return await action();
    } finally {
      setBusy((current) => {
        const next = new Set(current);
        next.delete(root);
        return next;
      });
    }
  }, []);

  const { rename: renameMutation, setPinned: pinMutation, deleteIndex: deleteIndexMutation, remove: removeMutation, setSettings } = mutations;
  const { refetch } = query;

  const reload = useCallback(async () => {
    await refetch();
  }, [refetch]);

  const rename = useCallback(
    (root: string, name: string) =>
      withBusy(root, async () => {
        await renameMutation.mutateAsync({ root, name }).catch((error: unknown) => {
          toast.error("Couldn't rename that folder", { message: message(error) });
        });
      }),
    [renameMutation, withBusy]
  );

  const setPinned = useCallback(
    (root: string, pinned: boolean) =>
      withBusy(root, async () => {
        await pinMutation.mutateAsync({ root, pinned }).catch((error: unknown) => {
          toast.error(pinned ? "Couldn't pin that folder" : "Couldn't unpin that folder", { message: message(error) });
        });
      }),
    [pinMutation, withBusy]
  );

  const deleteIndex = useCallback(
    (root: string) =>
      withBusy(root, async () => {
        await deleteIndexMutation.mutateAsync(root).catch((error: unknown) => {
          toast.error("Couldn't delete that folder's index", { message: message(error) });
        });
      }),
    [deleteIndexMutation, withBusy]
  );

  const remove = useCallback(
    (root: string, deleteThreads: boolean) =>
      withBusy(root, async () => {
        try {
          await removeMutation.mutateAsync({ root, deleteThreads });
          return true;
        } catch (error) {
          toast.error("Couldn't remove that folder", { message: message(error) });
          return false;
        }
      }),
    [removeMutation, withBusy]
  );

  const saveSettings = useCallback(
    (root: string, instructions: string, spaceId: string) =>
      withBusy(root, async () => {
        try {
          await setSettings.mutateAsync({ root, instructions, spaceId });
          return true;
        } catch (error) {
          toast.error("Couldn't save that folder's settings", { message: message(error) });
          return false;
        }
      }),
    [setSettings, withBusy]
  );

  return {
    folders: shown,
    home: list?.home ?? null,
    error: query.error ? query.error.message : null,
    busy,
    reload,
    rename,
    setPinned,
    deleteIndex,
    remove,
    saveSettings,
  };
}
