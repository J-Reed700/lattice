import { useCallback, useEffect, useMemo, useState } from 'react';

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { VaultAPI } from '@/lib/api';
import { queryClient } from '@/lib/queryClient';
import { useExplorerStore, type FolderIndexStatus } from '@/stores/explorerStore';
import { toast } from '@/stores/toastStore';

import { INDEX_STATUS_EVENT } from './indexProgress';

/** The space a folder files its threads in until another is chosen. */
export const GENERAL_SPACE_ID = 'space_general';

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

/** Pinned first, then the most recently opened: the backend's order. */
export function sortFolders(folders: ExplorerFolder[]): ExplorerFolder[] {
  return [...folders].sort(
    (a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOpenedAt.localeCompare(a.lastOpenedAt) || a.root.localeCompare(b.root)
  );
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

/**
 * The folders list, loaded once and kept fresh by actions and by the open
 * folder's status events, so a row that is indexing moves live.
 */
export function useExplorerFolders(): ExplorerFolders {
  const forgetThread = useExplorerStore((state) => state.forgetThread);
  const [folders, setFolders] = useState<ExplorerFolder[] | null>(null);
  const [home, setHome] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<ReadonlySet<string>>(() => new Set());
  const [live, setLive] = useState<Record<string, FolderIndexStatus>>({});

  const reload = useCallback(async () => {
    const result = await VaultAPI.explorerFoldersList();
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setError(null);
    setHome(result.data.home);
    setFolders(result.data.folders);
    // The list is newer than any event seen before it.
    setLive({});
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | undefined;
    void listen<FolderIndexStatus>(INDEX_STATUS_EVENT, (event) => {
      setLive((current) => ({ ...current, [event.payload.root]: event.payload }));
    }).then((fn) => {
      if (mounted) unlisten = fn;
      else fn();
    });
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  const shown = useMemo(
    () =>
      folders?.map((folder) => {
        const status = live[folder.root];
        return status ? { ...folder, index: summaryFromStatus(status, folder.index) } : folder;
      }) ?? null,
    [folders, live]
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

  const rename = useCallback(
    (root: string, name: string) =>
      withBusy(root, async () => {
        const result = await VaultAPI.explorerFolderRename(root, name);
        if (!result.ok) toast.error("Couldn't rename that folder", { message: result.error });
        await reload();
      }),
    [reload, withBusy]
  );

  const setPinned = useCallback(
    (root: string, pinned: boolean) =>
      withBusy(root, async () => {
        // Moves at once; the reload confirms it.
        setFolders((current) => current && sortFolders(current.map((folder) => (folder.root === root ? { ...folder, pinned } : folder))));
        const result = await VaultAPI.explorerFolderSetPinned(root, pinned);
        if (!result.ok) toast.error(pinned ? "Couldn't pin that folder" : "Couldn't unpin that folder", { message: result.error });
        await reload();
      }),
    [reload, withBusy]
  );

  const deleteIndex = useCallback(
    (root: string) =>
      withBusy(root, async () => {
        const result = await VaultAPI.explorerFolderDeleteIndex(root);
        if (!result.ok) toast.error("Couldn't delete that folder's index", { message: result.error });
        await reload();
      }),
    [reload, withBusy]
  );

  const remove = useCallback(
    (root: string, deleteThreads: boolean) =>
      withBusy(root, async () => {
        const result = await VaultAPI.explorerFolderRemove(root, deleteThreads);
        if (!result.ok) {
          toast.error("Couldn't remove that folder", { message: result.error });
          await reload();
          return false;
        }
        if (result.data > 0) {
          forgetThread(root);
          void queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
        }
        setFolders((current) => current?.filter((folder) => folder.root !== root) ?? null);
        await reload();
        return true;
      }),
    [forgetThread, reload, withBusy]
  );

  const saveSettings = useCallback(
    (root: string, instructions: string, spaceId: string) =>
      withBusy(root, async () => {
        const result = await VaultAPI.explorerFolderSetSettings(root, instructions, spaceId);
        if (!result.ok) {
          toast.error("Couldn't save that folder's settings", { message: result.error });
          return false;
        }
        // Moved threads change space in the Chat sidebar too.
        if (result.data > 0) void queryClient.invalidateQueries({ queryKey: conversationKeys.all });
        await reload();
        return true;
      }),
    [reload, withBusy]
  );

  return { folders: shown, home, error, busy, reload, rename, setPinned, deleteIndex, remove, saveSettings };
}
