import { useMutation } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/** Empties the search cache. Answers with the command's result. */
export function useClearCacheMutation() {
  const api = useApi();
  return useMutation({ mutationFn: () => api.clearCache() });
}
