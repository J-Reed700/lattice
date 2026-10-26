import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/** Whether a model's files are on disk, checked again every 15 seconds. */
export function useModelDownloadedQuery(modelId: string | undefined, enabled: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: ['utility-model-download-ready', modelId],
    enabled,
    queryFn: async () => {
      const result = await api.isModelDownloaded(modelId!);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 15_000,
    refetchInterval: 15_000,
  });
}
