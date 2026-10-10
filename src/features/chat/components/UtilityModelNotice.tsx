import { Link } from 'react-router';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useModelDownloadedQuery } from '@/features/model/hooks/useModelDownloadedQuery';

export function UtilityModelNotice() {
  const { downloadedModels, isLoading, error } = useDownloadedModels();
  const utility = downloadedModels.find(model => model.is_active_for_utility);
  const readiness = useModelDownloadedQuery(utility?.model_id, Boolean(utility && utility.backend !== 'ollama'));

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
