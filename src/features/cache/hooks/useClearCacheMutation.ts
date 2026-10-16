import { useMutation } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

/** Empties the search cache. Answers with the command's result. */
export function useClearCacheMutation() {
  return useMutation({ mutationFn: () => VaultAPI.clearCache() });
}
