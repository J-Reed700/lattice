/**
 * The model that hears a voice conversation, offered where the voice that
 * answers it is. Lives apart from the read-aloud settings because its status
 * comes from transcription, not speech.
 */

import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Check } from 'lucide-react';

import { SettingsRow, SettingsSection } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { useAggregatedDownloads } from '@/features/model/hooks/useAggregatedDownloads';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { TRANSCRIPTION_STATUS_QUERY_KEY, useTranscriptionStatusQuery } from '@/features/transcription/hooks/useTranscriptionStatus';
import { useToastStore } from '@/stores/toastStore';
import { formatBytes } from '@/utils/formatters';

import { PRIMARY_BUTTON_CLASS } from './settingsStyles';
import { startModelDownload } from '../hooks/startModelDownload';

export function ListeningModelSection() {
  const api = useApi();
  const queryClient = useQueryClient();
  const addToast = useToastStore((state) => state.addToast);
  const { getDownloadsByModel } = useDownloadState();
  const [starting, setStarting] = useState(false);

  const { data: status, isLoading, isError, refetch } = useTranscriptionStatusQuery();
  const offered = status?.voiceModel;
  const ready = status?.modelReady ?? false;
  const { aggregated, standalone } = useAggregatedDownloads(offered ? getDownloadsByModel(offered.modelId) : []);
  const download = aggregated[0] ?? standalone[0] ?? null;
  const downloading = download !== null && ['Pending', 'Downloading', 'Paused'].includes(download.state);
  const percent = Math.round(download?.percentage ?? 0);

  useEffect(() => {
    if (download?.state === 'Completed' && !ready) {
      void queryClient.invalidateQueries({ queryKey: TRANSCRIPTION_STATUS_QUERY_KEY });
    }
  }, [download?.state, ready, queryClient]);

  const startDownload = async () => {
    if (!offered) return;
    setStarting(true);
    try {
      await startModelDownload(
        {
          metadata: { id: offered.modelId, name: offered.modelName, category: 'Transcription', embedding_compatibility: null },
          addToast,
          queryClient,
        },
        api,
      );
      await queryClient.invalidateQueries({ queryKey: TRANSCRIPTION_STATUS_QUERY_KEY });
    } finally {
      setStarting(false);
    }
  };

  return (
    <SettingsSection title="Voice conversations">
      {isError ? (
        <p role="alert" className="py-3 text-sm text-danger-fg">
          Couldn&apos;t check the listening model.{' '}
          <button type="button" className="underline" onClick={() => void refetch()}>Try again</button>
        </p>
      ) : (
        <SettingsRow
          label={status?.modelName ?? offered?.modelName ?? 'Whisper'}
          hint={offered
            ? `Hears you when you press the mic in a chat. ${formatBytes(offered.downloadBytes)} once, then it works offline. ${offered.license} licence.`
            : undefined}
        >
          {isLoading ? null : ready ? (
            <span className="inline-flex items-center gap-1.5 text-sm text-text-secondary">
              <Check className="h-4 w-4 text-accent" aria-hidden="true" />
              Downloaded
            </span>
          ) : downloading ? (
            <div className="flex w-full max-w-[240px] flex-col gap-1" role="status" aria-label="Downloading the listening model">
              <progress className="h-1.5 w-full accent-accent" max={100} value={percent} />
              <span className="text-xs tabular-nums text-text-muted">
                {download.state === 'Paused' ? 'Paused' : `Downloading… ${percent}%`}
              </span>
            </div>
          ) : (
            <button
              type="button"
              className={PRIMARY_BUTTON_CLASS}
              aria-label="Download the listening model"
              disabled={starting || !offered}
              onClick={() => void startDownload()}
            >
              {starting ? 'Starting…' : 'Download'}
            </button>
          )}
        </SettingsRow>
      )}
    </SettingsSection>
  );
}
