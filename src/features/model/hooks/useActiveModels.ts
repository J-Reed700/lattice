import { keepPreviousData, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { downloadedModelsQueryOptions } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import type { ActiveModels } from '@/lib/bindings';

export const ACTIVE_MODELS_QUERY_KEY = ['active-models'] as const;

/**
 * What a chat on the Settings default and utility work run on, as the backend
 * decides it. The answer is worked out from Settings and the downloaded rows,
 * so it is fetched again whenever either changes.
 */
export function useActiveModels() {
  const api = useApi();
  const settingsQuery = useSettingsQuery();
  const modelsQuery = useQuery(downloadedModelsQueryOptions(api));
  const { data, isPending, error } = useQuery<ActiveModels>({
    queryKey: [...ACTIVE_MODELS_QUERY_KEY, settingsQuery.dataUpdatedAt, modelsQuery.dataUpdatedAt],
    queryFn: async () => {
      const result = await api.getActiveModels();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    placeholderData: keepPreviousData,
    enabled: settingsQuery.data !== undefined && modelsQuery.data !== undefined,
  });
  return { activeModels: data ?? null, isPending, error };
}
