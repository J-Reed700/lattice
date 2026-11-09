export function useRetry<A extends unknown[], R>(
  _action: (...args: A) => Promise<R>,
): { run: (...args: A) => Promise<R | undefined>; retry: () => void; busy: boolean; error: string | null } {
  return { run: () => Promise.resolve(undefined), retry: () => {}, busy: false, error: null };
}
