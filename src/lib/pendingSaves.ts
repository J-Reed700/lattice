/** Lifecycle callbacks only; repositories remain the owners of persisted data. */
const pendingSaves = new Set<() => Promise<boolean>>();

export function registerPendingSave(save: () => Promise<boolean>): () => void {
  pendingSaves.add(save);
  return () => { pendingSaves.delete(save); };
}

export async function flushPendingSaves(): Promise<boolean> {
  const results = await Promise.allSettled([...pendingSaves].map((save) => Promise.resolve().then(save)));
  return results.every((result) => result.status === 'fulfilled' && result.value);
}
