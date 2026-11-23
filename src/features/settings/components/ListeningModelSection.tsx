/**
 * The model that hears a voice conversation, offered where the voice that
 * answers it is. Lives apart from the read-aloud settings because its status
 * comes from transcription, not speech.
 */

import { useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Check } from 'lucide-react';

import { SettingsRow, SettingsSection } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { DownloadFailure } from '@/features/model/components/Downloads/DownloadFailure';
import { InlineDownloadStatus } from '@/features/model/components/Downloads/InlineDownloadStatus';
import { InstalledDownload } from '@/features/model/components/Downloads/InstalledDownload';
import { useAggregatedDownloads } from '@/features/model/hooks/useAggregatedDownloads';
import { useDownloadPhase } from '@/features/model/hooks/useDownloadPhase';
import { DOWNLOADS_QUERY_KEY } from '@/features/model/hooks/useDownloads';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { useRemoveDownloadedModel } from '@/features/model/hooks/useRemoveDownloadedModel';
import { TRANSCRIPTION_STATUS_QUERY_KEY, useTranscriptionStatusQuery } from '@/features/transcription/hooks/useTranscriptionStatus';
import { useToastStore } from '@/stores/toastStore';
import { formatBytes } from '@/utils/formatters';

import { PRIMARY_BUTTON_CLASS } from './settingsStyles';
import { startModelDownload } from '../hooks/startModelDownload';

export function ListeningModelSection() {
  const api = useApi();
  const queryClient = useQueryClient();
  const addToast = useToastStore((state) => state.addToast);
  const { getDownloadsByModel, downloadsSettled } = useDownloadState();
  const removeModel = useRemoveDownloadedModel();
  const [starting, setStarting] = useState(false);

  const { data: status, isLoading, isError, refetch, isFetchedAfterMount } = useTranscriptionStatusQuery({ refetchOnMount: 'always' });
  const offered = status?.voiceModel;
  const ready = status?.modelReady ?? false;
  // The catalog id of the Whisper model in use, which may not be the one offered.
  const inUseModelId = status?.modelId ?? null;
  const { aggregated, standalone } = useAggregatedDownloads(offered ? getDownloadsByModel(offered.modelId) : []);
  const download = aggregated[0] ?? standalone[0] ?? null;
  const phase = useDownloadPhase({ ready, download, poll: refetch });

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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: DOWNLOADS_QUERY_KEY }),
        queryClient.invalidateQueries({ queryKey: TRANSCRIPTION_STATUS_QUERY_KEY }),
      ]);
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
          {isLoading ? null : ready && inUseModelId ? (
            <InstalledDownload
              name={status?.modelName ?? offered?.modelName ?? 'the listening model'}
              consequence="The mic in a chat can't hear you until you download it again."
              size={offered?.modelId === inUseModelId ? formatBytes(offered.downloadBytes) : undefined}
              onRemove={() => removeModel(inUseModelId, TRANSCRIPTION_STATUS_QUERY_KEY)}
            />
          ) : ready ? (
            <span className="inline-flex items-center gap-1.5 text-sm text-text-secondary">
              <Check className="h-4 w-4 text-accent" aria-hidden="true" />
              Downloaded
            </span>
          ) : (phase === 'live' || phase === 'finishing') && download ? (
            <InlineDownloadStatus download={download} finishing={phase === 'finishing'} label="Downloading the listening model" />
          ) : (
            <div className="flex flex-col items-end gap-1">
              <button
                type="button"
                className={PRIMARY_BUTTON_CLASS}
                aria-label="Download the listening model"
                disabled={starting || !offered || !isFetchedAfterMount || !downloadsSettled}
                onClick={() => void startDownload()}
              >
                {starting ? 'Starting…' : 'Download'}
              </button>
              <DownloadFailure download={download} />
            </div>
          )}
        </SettingsRow>
      )}
    </SettingsSection>
  );
}
