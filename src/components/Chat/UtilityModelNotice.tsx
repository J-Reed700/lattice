import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import VaultAPI from '@/lib/api';

export function UtilityModelNotice() {
  const { downloadedModels, isLoading, error } = useDownloadedModels();
  const utility = downloadedModels.find(model => model.is_active_for_utility);
  const readiness = useQuery({
    queryKey: ['utility-model-download-ready', utility?.model_id],
    enabled: Boolean(utility && utility.backend !== 'ollama'),
    queryFn: async () => {
      const result = await VaultAPI.isModelDownloaded(utility!.model_id);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 15_000,
    refetchInterval: 15_000,
  });

  if (isLoading) return null;
  let message: string;
  if (error || readiness.error) {
    message = 'Utility model availability could not be checked.';
  } else if (!utility) {
    message = 'No downloaded utility model is available for use. Document search uses basic query planning. Download or select a utility model to enable AI query planning.';
  } else if (utility.backend === 'ollama' || readiness.data === true) {
    return null;
  } else if (readiness.isPending) {
    return null;
  } else {
    message = `The selected utility model (${utility.model_name}) is not fully downloaded or its file is missing. Document search uses basic query planning until it is available.`;
  }

  return (
    <div role="status" className="border-b border-border-subtle px-6 py-3 text-sm text-text-secondary">
      {message}{' '}
      <Link to="/settings?tab=models" className="text-accent underline underline-offset-2">
        Open model catalog
      </Link>
    </div>
  );
}
