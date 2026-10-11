import { useCallback, useEffect, useMemo } from 'react';

import {
  queryOptions,
  skipToken,
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';

import type { FolderIndexStatus } from '@/features/explorer/stores/explorerStore';
import { isJobFinished, useJobs, useJobStatus, type JobDto } from '@/features/jobs/api';
import { VaultAPI } from '@/lib/api';
import type { ExplorerFolderListDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ApiResult } from '@/types';
import { unwrapApiResult } from '@/types/api/result';

/** The job kind of a folder index build; its activity is a `FolderIndexStatusDto`. */
export const FOLDER_INDEX_JOB = 'explorer.folder_index';
const FOLDER_INDEX_JOBS = [FOLDER_INDEX_JOB] as const;

export const explorerKeys = {
  all: ['explorer'] as const,
  folders: ['explorer', 'folders'] as const,
  indexStatus: (root: string) => ['explorer', 'index-status', root] as const,
};

export const explorerFoldersQueryOptions = () =>
  queryOptions({
    queryKey: explorerKeys.folders,
    queryFn: async () => unwrapApiResult(await VaultAPI.explorerFoldersList()),
    staleTime: 15_000,
  });

/** "Your folders": pinned first, then the most recently opened. */
export function useExplorerFoldersQuery() {
  return useQuery(explorerFoldersQueryOptions());
}

/** Canonicalises a folder the user picked; it must be an existing directory. */
export const resolveExplorerRoot = (path: string) => VaultAPI.explorerResolveRoot(path);

/**
 * Binds a conversation to a folder and files it in the folder's space. The
 * caller updates the conversation's cached detail and lists.
 */
export const bindThreadToFolder = (conversationId: string, root: string) =>
  VaultAPI.setConversationExplorerRoot(conversationId, root);

/**
 * The folders list's writes. Each refreshes the list; the ones that move or
 * delete threads refresh the conversation lists too.
 */
export function useExplorerFolderMutations() {
  const client = useQueryClient();
  const refreshFolders = () => client.invalidateQueries({ queryKey: explorerKeys.folders });
  const rename = useMutation({
    mutationFn: async ({ root, name }: { root: string; name: string }) =>
      unwrapApiResult(await VaultAPI.explorerFolderRename(root, name)),
    onSettled: refreshFolders,
  });
  const setPinned = useMutation({
    mutationFn: async ({ root, pinned }: { root: string; pinned: boolean }) =>
      unwrapApiResult(await VaultAPI.explorerFolderSetPinned(root, pinned)),
    // Moves at once; the refresh confirms it.
    onMutate: ({ root, pinned }) => {
      client.setQueryData<ExplorerFolderListDto>(explorerKeys.folders, (list) => list && {
        ...list,
        folders: sortFolders(list.folders.map((folder) => (folder.root === root ? { ...folder, pinned } : folder))),
      });
    },
    onSettled: refreshFolders,
  });
  const setSettings = useMutation({
    mutationFn: async ({ root, instructions, spaceId }: { root: string; instructions: string; spaceId: string }) =>
      unwrapApiResult(await VaultAPI.explorerFolderSetSettings(root, instructions, spaceId)),
    onSuccess: async (moved) => {
      // Moved threads change space in the Chat sidebar and the thread bar.
      if (moved > 0) await client.invalidateQueries({ queryKey: conversationKeys.all });
      await refreshFolders();
    },
  });
  const deleteIndex = useMutation({
    mutationFn: async (root: string) => unwrapApiResult(await VaultAPI.explorerFolderDeleteIndex(root)),
    onSettled: refreshFolders,
  });
  const remove = useMutation({
    mutationFn: async ({ root, deleteThreads }: { root: string; deleteThreads: boolean }) =>
      unwrapApiResult(await VaultAPI.explorerFolderRemove(root, deleteThreads)),
    onSuccess: (deleted, { root }) => {
      client.setQueryData<ExplorerFolderListDto>(explorerKeys.folders, (list) => list && {
        ...list,
        folders: list.folders.filter((folder) => folder.root !== root),
      });
      if (deleted > 0) void client.invalidateQueries({ queryKey: conversationKeys.lists });
    },
    onSettled: refreshFolders,
  });
  const setLastThread = useMutation({
    mutationFn: async ({ root, conversationId }: { root: string; conversationId: string }) =>
      unwrapApiResult(await VaultAPI.explorerFolderSetLastThread(root, conversationId)),
    onSuccess: async (_result, { root, conversationId }) => {
      const list = client.getQueryData<ExplorerFolderListDto>(explorerKeys.folders);
      if (list?.folders.some((folder) => folder.root === root)) {
        client.setQueryData<ExplorerFolderListDto>(explorerKeys.folders, {
          ...list,
          folders: list.folders.map((folder) => (folder.root === root ? { ...folder, lastThreadId: conversationId } : folder)),
        });
      } else {
        // Remembering a thread lists a folder that was not listed yet.
        await refreshFolders();
      }
    },
  });
  return { rename, setPinned, setSettings, deleteIndex, remove, setLastThread };
}

/** Pinned first, then the most recently opened: the backend's order. */
export function sortFolders<T extends { root: string; pinned: boolean; lastOpenedAt: string }>(folders: T[]): T[] {
  return [...folders].sort(
    (a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOpenedAt.localeCompare(a.lastOpenedAt) || a.root.localeCompare(b.root)
  );
}

/** The status a folder build last saved, when it is running or has ended with one. */
export function buildStatus(job: JobDto): FolderIndexStatus | null {
  if (job.kind !== FOLDER_INDEX_JOB || job.status === 'pending' || job.status === 'cancelled') return null;
  const status = job.activity as FolderIndexStatus | null;
  return status && typeof status.root === 'string' ? status : null;
}

/** The folder a build walks, whatever its state. */
function buildRoot(job: JobDto): string | null {
  const root = (job.activity as { root?: unknown } | null)?.root;
  return typeof root === 'string' ? root : null;
}

/**
 * Carries the open folder's `FolderIndexStatusDto` when the watcher's updates
 * move it, after its build is done. Builds report through their jobs.
 */
export const FOLDER_CHANGED_EVENT = 'explorer://folder-changed';

/**
 * Puts a watcher update into the cache: the folder's status, and the folders
 * list, whose counts it changed.
 */
export function applyFolderChange(client: QueryClient, status: FolderIndexStatus): void {
  client.setQueryData(explorerKeys.indexStatus(status.root), status);
  void client.invalidateQueries({ queryKey: explorerKeys.folders });
}

/**
 * Puts the folder builds' statuses into the cache, one entry per folder: the
 * builds running when the page mounts, then each change the jobs report. A
 * build that is waiting its turn or was cancelled drops its entry, so the
 * folder shows what its index on disk holds. The open folder's watcher
 * updates arrive on their own event. Mounted once, by the Explorer page;
 * everything that shows a status reads the cache.
 */
export function useExplorerIndexStatusEvents(): void {
  const client = useQueryClient();
  const builds = useJobs(FOLDER_INDEX_JOBS);
  const apply = useCallback(
    (job: JobDto) => {
      const status = buildStatus(job);
      if (status) {
        client.setQueryData(explorerKeys.indexStatus(status.root), status);
        return;
      }
      const root = buildRoot(job);
      if (root && (!isJobFinished(job) || job.status === 'cancelled')) {
        client.removeQueries({ queryKey: explorerKeys.indexStatus(root), exact: true });
      }
    },
    [client]
  );
  useJobStatus(apply);
  const running = builds.data;
  useEffect(() => {
    for (const job of running ?? []) if (job.status === 'running') apply(job);
  }, [running, apply]);
  useEffect(() => {
    const listening = listen<FolderIndexStatus>(FOLDER_CHANGED_EVENT, (event) => applyFolderChange(client, event.payload))
      .catch(() => () => undefined);
    return () => void listening.then((unlisten) => unlisten());
  }, [client]);
}

const statusQuery = (root: string) =>
  queryOptions<FolderIndexStatus>({
    queryKey: explorerKeys.indexStatus(root),
    // Written by events and index commands, never fetched.
    queryFn: skipToken,
    staleTime: Infinity,
  });

/** A folder's index status as the backend last reported it; `null` until it has. */
export function useExplorerIndexStatus(root: string | null): FolderIndexStatus | null {
  const { data } = useQuery(statusQuery(root ?? ''));
  return root ? data ?? null : null;
}

/** The statuses reported for `roots` after `since` (ms); older ones are what a list read later already says. */
export function useExplorerIndexStatuses(roots: readonly string[], since: number): ReadonlyMap<string, FolderIndexStatus> {
  const combine = useCallback(
    (results: Array<{ data?: FolderIndexStatus; dataUpdatedAt: number }>) => {
      const live = new Map<string, FolderIndexStatus>();
      results.forEach((result, index) => {
        if (result.data && result.dataUpdatedAt >= since) live.set(roots[index], result.data);
      });
      return live;
    },
    [roots, since]
  );
  return useQueries({ queries: roots.map((root) => statusQuery(root)), combine });
}

/** A command that failed outright, shown the way a failed run is. */
function failedIndex(root: string, message: string): FolderIndexStatus {
  return {
    root,
    indexRoot: root,
    state: 'error',
    filesTotal: 0,
    filesIndexed: 0,
    passagesTotal: 0,
    passagesEmbedded: 0,
    passagesPerSecond: null,
    etaSeconds: null,
    message,
  };
}

/**
 * Runs an index command and caches its result, unless a status arrived while
 * it ran: that one is newer than the command's snapshot, and a small folder
 * can finish indexing before the command's reply is back.
 */
async function runIndexCommand(
  client: QueryClient,
  command: () => Promise<ApiResult<FolderIndexStatus>>,
  root: string,
  stillWanted: () => boolean
): Promise<void> {
  const key = explorerKeys.indexStatus(root);
  const before = client.getQueryState(key)?.dataUpdateCount ?? 0;
  const result = await command();
  if (!stillWanted()) return;
  if (!result.ok) client.setQueryData(key, failedIndex(root, result.error));
  else if ((client.getQueryState(key)?.dataUpdateCount ?? 0) === before) client.setQueryData(key, result.data);
}

/** Opening, rebuilding and closing the open folder's index; results land in the status cache. */
export function useExplorerIndexCommands() {
  const client = useQueryClient();
  return useMemo(() => ({
    /** Opens the folder's index and starts or resumes indexing; the folder joins the list. */
    open: async (root: string, stillWanted: () => boolean = () => true) => {
      await runIndexCommand(client, () => VaultAPI.explorerIndexOpen(root), root, stillWanted);
      void client.invalidateQueries({ queryKey: explorerKeys.folders });
    },
    /** Wipes the index and builds it again; `retry` resumes a failed one instead. */
    rebuild: (root: string, retry: boolean) =>
      runIndexCommand(
        client,
        () => (retry ? VaultAPI.explorerIndexOpen(root) : VaultAPI.explorerIndexRebuild(root)),
        root,
        () => true
      ),
    /** Closes the open folder's index, waiting for the batch in flight and a save. */
    close: async (root: string | null) => {
      await VaultAPI.explorerIndexClose();
      // A closed index reports nothing more; the folders list reads it from disk.
      if (root) client.removeQueries({ queryKey: explorerKeys.indexStatus(root), exact: true });
      void client.invalidateQueries({ queryKey: explorerKeys.folders });
    },
  }), [client]);
}
