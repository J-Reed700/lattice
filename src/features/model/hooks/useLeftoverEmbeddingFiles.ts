/**
 * Files the previous (safetensors) embedding runtime left in an embedding
 * model folder, and removing exactly them on request.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';
import type { LeftoverEmbeddingFilesDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import { formatBytes } from '@/utils/formatters';

export const LEFTOVER_EMBEDDING_FILES_QUERY_KEY = ['leftover-embedding-files'] as const;

async function loadLeftovers(api: ApiClient = VaultAPI): Promise<LeftoverEmbeddingFilesDto> {
  const result = await api.listLeftoverEmbeddingFiles();
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLeftoverEmbeddingFiles() {
  const api = useApi();
  const queryClient = useQueryClient();
  const leftovers = useQuery({
    queryKey: LEFTOVER_EMBEDDING_FILES_QUERY_KEY,
    queryFn: () => loadLeftovers(api),
  });
  const remove = useMutation({
    mutationFn: async () => {
      const result = await api.removeLeftoverEmbeddingFiles();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    onSuccess: (removed) => {
      toast.success(`Removed ${formatBytes(removed.total_bytes)} of old embedding files`);
    },
    onError: (error) => {
      toast.error("Couldn't remove the old embedding files", {
        message: error instanceof Error ? error.message : String(error),
      });
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: LEFTOVER_EMBEDDING_FILES_QUERY_KEY }),
  });
  return { leftovers: leftovers.data, remove };
}
