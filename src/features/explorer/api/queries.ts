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

export function useExplorerFoldersQuery() {
  return useQuery(explorerFoldersQueryOptions());
}

export const resolveExplorerRoot = (path: string) => VaultAPI.explorerResolveRoot(path);

export const bindThreadToFolder = (conversationId: string, root: string) =>
  VaultAPI.setConversationExplorerRoot(conversationId, root);

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
        await refreshFolders();
      }
    },
  });
  return { rename, setPinned, setSettings, deleteIndex, remove, setLastThread };
}

export function sortFolders<T extends { root: string; pinned: boolean; lastOpenedAt: string }>(folders: T[]): T[] {
  return [...folders].sort(
    (a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOpenedAt.localeCompare(a.lastOpenedAt) || a.root.localeCompare(b.root)
  );
}

export function buildStatus(job: JobDto): FolderIndexStatus | null {
  if (job.kind !== FOLDER_INDEX_JOB || job.status === 'pending' || job.status === 'cancelled') return null;
  const status = job.activity as FolderIndexStatus | null;
  return status && typeof status.root === 'string' ? status : null;
}

function buildRoot(job: JobDto): string | null {
  const root = (job.activity as { root?: unknown } | null)?.root;
  return typeof root === 'string' ? root : null;
}

export const FOLDER_CHANGED_EVENT = 'explorer://folder-changed';

export function applyFolderChange(client: QueryClient, status: FolderIndexStatus): void {
  client.setQueryData(explorerKeys.indexStatus(status.root), status);
  void client.invalidateQueries({ queryKey: explorerKeys.folders });
}

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
    queryFn: skipToken,
    staleTime: Infinity,
  });

export function useExplorerIndexStatus(root: string | null): FolderIndexStatus | null {
  const { data } = useQuery(statusQuery(root ?? ''));
  return root ? data ?? null : null;
}

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

export function useExplorerIndexCommands() {
  const client = useQueryClient();
  return useMemo(() => ({
    open: async (root: string, stillWanted: () => boolean = () => true) => {
      await runIndexCommand(client, () => VaultAPI.explorerIndexOpen(root), root, stillWanted);
      void client.invalidateQueries({ queryKey: explorerKeys.folders });
    },
    rebuild: (root: string, retry: boolean) =>
      runIndexCommand(
        client,
        () => (retry ? VaultAPI.explorerIndexOpen(root) : VaultAPI.explorerIndexRebuild(root)),
        root,
        () => true
      ),
    close: async (root: string | null) => {
      await VaultAPI.explorerIndexClose();
      if (root) client.removeQueries({ queryKey: explorerKeys.indexStatus(root), exact: true });
      void client.invalidateQueries({ queryKey: explorerKeys.folders });
    },
  }), [client]);
}

const FILE_CACHE = { staleTime: 60_000, gcTime: 30 * 60_000 } as const;

export const explorerDirKey = (root: string, path: string) => ['explorer', 'dir', root, path] as const;

export function useExplorerDirListings(root: string, dirs: readonly string[]) {
  return useQueries({
    queries: dirs.map((path) => ({
      queryKey: explorerDirKey(root, path),
      queryFn: async () => unwrapApiResult(await VaultAPI.explorerListDir(root, path)),
      staleTime: 10_000,
    })),
  });
}

export function useExplorerFileQuery(root: string, path: string | null) {
  return useQuery({
    queryKey: ['explorer', 'file', root, path],
    queryFn: async () => unwrapApiResult(await VaultAPI.explorerReadFile(root, path!)),
    enabled: Boolean(path),
    ...FILE_CACHE,
  });
}

export function useExplorerLocateQuery(root: string, path: string) {
  return useQuery({
    queryKey: ['explorer', 'locate', root, path],
    queryFn: async () => {
      const found = unwrapApiResult(await VaultAPI.explorerLocateFile(root, path));
      return { exists: found.includes(path), matches: found.filter((match) => match !== path) };
    },
    ...FILE_CACHE,
  });
}

export function useExplorerSearchQuery(root: string, query: string, enabled: boolean) {
  return useQuery({
    queryKey: ['explorer', 'search', root, query],
    queryFn: async () => unwrapApiResult(await VaultAPI.explorerSearch(root, query, { maxResults: 200 })),
    enabled,
    staleTime: 5_000,
  });
}
