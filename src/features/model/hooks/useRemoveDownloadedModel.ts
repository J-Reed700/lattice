import { useCallback } from 'react';

import { type QueryKey, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';

import { DOWNLOADED_MODELS_QUERY_KEY, downloadedModelsQueryOptions } from './useDownloadedModels';
import { DOWNLOADS_QUERY_KEY } from './useDownloads';

/**
 * Deletes a downloaded catalog model and its files with the same delete the
 * model list uses, then refreshes the download list before `statusKey`, so a
 * download that finished a moment ago never reads as "Finishing…".
 */
export function useRemoveDownloadedModel(): (catalogModelId: string, statusKey: QueryKey) => Promise<void> {
  const api = useApi();
  const queryClient = useQueryClient();
  return useCallback(
    async (catalogModelId, statusKey) => {
      const models = await queryClient.fetchQuery({ ...downloadedModelsQueryOptions(api), staleTime: 0 });
      const record = models.find((model) => model.model_id === catalogModelId);
      // No record means it is already gone: only the views need refreshing.
      if (record) {
        const result = await api.deleteDownloadedModel(record.id, true);
        if (!result.ok) throw new Error(result.error);
      }
      // The delete removed the model's download sessions without telling the
      // UI. Read the list first: a status that flips to "not installed" while
      // a just-completed session is still listed would read as "Finishing…".
      await queryClient.invalidateQueries({ queryKey: DOWNLOADS_QUERY_KEY });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: DOWNLOADED_MODELS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: statusKey }),
      ]);
    },
    [api, queryClient],
  );
}
