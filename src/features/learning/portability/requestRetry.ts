/**
 * Retrying portability requests safely. While a request's payload is
 * unchanged it keeps the ids made for it, so retrying after a lost response
 * replays the recorded result instead of running the request twice.
 */
export type RetryEntry<T> = { fingerprint: string; request: T };

function newId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
    const random = Math.floor(Math.random() * 16);
    return (char === 'x' ? random : (random & 3) | 8).toString(16);
  });
}

/**
 * The request kept under `key` when `payload` is unchanged; otherwise a new
 * request, `payload` plus the fresh ids `extra` makes, kept under `key` for a
 * retry. Callers drop the entry once the request succeeds.
 */
export function idempotentEntry<K extends PropertyKey, P extends object, E extends object>(
  ref: { current: NoInfer<Partial<Record<K, RetryEntry<P & E>>>> },
  key: K,
  payload: P,
  extra: (id: () => string) => E,
): RetryEntry<P & E> {
  const fingerprint = JSON.stringify(payload);
  const kept = ref.current[key];
  if (kept?.fingerprint === fingerprint) return kept;
  const entry = { fingerprint, request: { ...payload, ...extra(newId) } };
  ref.current[key] = entry;
  return entry;
}

/** The text shown for a failed request. */
export const failureMessage = (cause: unknown, fallback: string) => cause instanceof Error ? cause.message : fallback;
