import { useState } from 'react';

import { Info, X } from 'lucide-react';
import { Link } from 'react-router';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useModelDownloadedQuery } from '@/features/model/hooks/useModelDownloadedQuery';
import { isUtilityModel } from '@/features/settings/components/modelRoles/roleConfig';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';

export function UtilityModelNotice() {
  const [dismissedMessage, setDismissedMessage] = useState<string | null>(null);
  const { downloadedModels, isLoading, error } = useDownloadedModels();
  const settings = useSettingsQuery();
  const llm = settings.data?.llm;
  const utility = downloadedModels.find(model => isUtilityModel(model, llm));
  const readiness = useModelDownloadedQuery(utility?.model_id, Boolean(utility && utility.backend !== 'ollama'));

  if (isLoading || settings.isPending || llm?.utilityModel) return null;
  let message: string;
  if (error || settings.error || readiness.error) {
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

  if (dismissedMessage === message) return null;

  return (
    <div
      role="status"
      className="flex items-center justify-between gap-3 border-b border-border-subtle bg-surface-sunken/60 px-6 py-1.5 text-xs text-text-tertiary"
    >
      <div className="flex min-w-0 items-center gap-2">
        <Info className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
        <p className="min-w-0 leading-snug">
          {message}{' '}
          <Link
            to="/settings?tab=models"
            className="font-medium text-accent underline-offset-2 transition-colors duration-fast hover:text-accent-hover hover:underline"
          >
            Open model catalog
          </Link>
        </p>
      </div>
      <button
        type="button"
        onClick={() => setDismissedMessage(message)}
        aria-label="Dismiss utility model notice"
        title="Dismiss"
        className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
      >
        <X className="h-3 w-3" strokeWidth={1.75} />
      </button>
    </div>
  );
}
