import { VaultAPI, type ApiClient } from '@/lib/api';
import type { WorkspaceNote } from '@/types/api/dailyNotes';
import { unwrapApiResult } from '@/types/api/result';

/** One editor owns one queue. Only its own acknowledged writes advance its drafts. */
export function createNoteSaveQueue(api: ApiClient = VaultAPI) {
  let tail: Promise<unknown> = Promise.resolve();
  const acknowledged = new Map<string, number>();
  return (snapshot: WorkspaceNote): Promise<WorkspaceNote> => {
    const write = async () => {
      const revision = Math.max(
        snapshot.revision,
        acknowledged.get(snapshot.id) ?? snapshot.revision,
      );
      const saved = unwrapApiResult(
        await api.updateWorkspaceNote({ ...snapshot, revision }),
      );
      acknowledged.set(saved.id, saved.revision);
      return saved;
    };
    const queued = tail.then(write);
    tail = queued.catch(() => undefined);
    return queued;
  };
}
