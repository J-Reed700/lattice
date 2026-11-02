/**
 * Read aloud settings: the two on-device downloads, which voice reads, and
 * how fast. The voice and pace are UI-only preferences, kept in the speech
 * feature's store.
 */

import { useEffect, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { Check, Pause, Volume2 } from 'lucide-react';

import { PageHeader, SettingsRow, SettingsSection, settingsFieldClass } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { useAggregatedDownloads } from '@/features/model/hooks/useAggregatedDownloads';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { SPEECH_STATUS_QUERY_KEY, stopSpeaking, useSpeaker, useSpeechPlayback, useSpeechStatusQuery } from '@/features/speech/hooks/useSpeech';
import { SPEEDS, useSpeechPrefsStore } from '@/features/speech/stores/speechPrefsStore';
import type { SpeechAccentDto } from '@/lib/bindings';
import { useToastStore } from '@/stores/toastStore';
import { formatBytes } from '@/utils/formatters';

import { GHOST_BUTTON_CLASS, PRIMARY_BUTTON_CLASS } from './settingsStyles';
import { startModelDownload } from '../hooks/startModelDownload';

const previewId = (voice: string) => `preview:${voice}`;

const ACCENTS: { accent: SpeechAccentDto; label: string }[] = [
  { accent: 'british', label: 'British English' },
  { accent: 'american', label: 'American English' },
];

interface DownloadRowProps {
  modelId: string;
  name: string;
  hint: string;
  ready: boolean;
}

/** One catalog entry: its download, its progress, or that it is here. */
function DownloadRow({ modelId, name, hint, ready }: DownloadRowProps) {
  const api = useApi();
  const queryClient = useQueryClient();
  const addToast = useToastStore((state) => state.addToast);
  const { getDownloadsByModel } = useDownloadState();
  const [starting, setStarting] = useState(false);

  const { aggregated, standalone } = useAggregatedDownloads(getDownloadsByModel(modelId));
  const download = aggregated[0] ?? standalone[0] ?? null;
  const downloading = download !== null && ['Pending', 'Downloading', 'Paused'].includes(download.state);
  const percent = Math.round(download?.percentage ?? 0);

  useEffect(() => {
    if (download?.state === 'Completed' && !ready) {
      void queryClient.invalidateQueries({ queryKey: SPEECH_STATUS_QUERY_KEY });
    }
  }, [download?.state, ready, queryClient]);

  const startDownload = async () => {
    setStarting(true);
    try {
      await startModelDownload(
        {
          metadata: { id: modelId, name, category: 'Speech', embedding_compatibility: null },
          addToast,
          queryClient,
        },
        api,
      );
      await queryClient.invalidateQueries({ queryKey: SPEECH_STATUS_QUERY_KEY });
    } finally {
      setStarting(false);
    }
  };

  return (
    <SettingsRow label={name} hint={hint}>
      {ready ? (
        <span className="inline-flex items-center gap-1.5 text-sm text-text-secondary">
          <Check className="h-4 w-4 text-accent" aria-hidden="true" />
          Downloaded
        </span>
      ) : downloading ? (
        <div className="flex w-full max-w-[240px] flex-col gap-1" role="status" aria-label={`Downloading ${name}`}>
          <progress className="h-1.5 w-full accent-accent" max={100} value={percent} />
          <span className="text-xs tabular-nums text-text-muted">
            {download.state === 'Paused' ? 'Paused' : `Downloading… ${percent}%`}
          </span>
        </div>
      ) : (
        <button type="button" className={PRIMARY_BUTTON_CLASS} aria-label={`Download ${name}`} disabled={starting} onClick={() => void startDownload()}>
          {starting ? 'Starting…' : 'Download'}
        </button>
      )}
    </SettingsRow>
  );
}

export function ReadAloudTab() {
  const { voice, speed, setVoice, setSpeed } = useSpeechPrefsStore();
  const speak = useSpeaker();
  const playback = useSpeechPlayback();
  const { data: speech, isError, refetch } = useSpeechStatusQuery();

  const preview = (id: string, name: string) => {
    if (playback.sourceId === previewId(id)) return;
    void speak(previewId(id), [`Hello, I'm ${name}. I can read your answers and lessons aloud.`], id);
  };

  return (
    <>
      <PageHeader title="Read aloud" description="Listen to chat answers and lessons, spoken on this computer." />

      <SettingsSection title="Voice model">
        {isError ? (
          <p role="alert" className="py-3 text-sm text-danger-fg">
            Couldn&apos;t check the voice model.{' '}
            <button type="button" className="underline" onClick={() => void refetch()}>Try again</button>
          </p>
        ) : speech ? (
          <>
            <DownloadRow
              modelId={speech.modelId}
              name={speech.modelName}
              ready={speech.modelReady}
              hint={`${formatBytes(speech.downloadBytes)} once, then it works offline. ${speech.license} licence.`}
            />
            <DownloadRow
              modelId={speech.voicePack.modelId}
              name={speech.voicePack.modelName}
              ready={speech.voicePack.ready}
              hint={`The American voices and the British men. ${formatBytes(speech.voicePack.downloadBytes)} more. ${speech.modelName} is not fetched again.`}
            />
          </>
        ) : (
          <SettingsRow label="Kokoro">{null}</SettingsRow>
        )}
      </SettingsSection>

      <SettingsSection title="Voice" description="Grades are hexgrad's estimate of each voice's training data, from A down to F.">
        <div role="radiogroup" aria-label="Voice">
          {speech && ACCENTS.map(({ accent, label }) => (
            <div key={accent} role="group" aria-label={label} className="mb-4">
              <h4 className="pt-3 text-xs font-medium text-text-secondary">{label}</h4>
              {speech.voices.filter((option) => option.accent === accent).map((option) => {
                const previewing = playback.sourceId === previewId(option.id) && playback.status !== 'idle';
                return (
                  <div key={option.id} className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5">
                    <label className="flex min-w-0 items-center gap-2.5 text-sm text-text-primary">
                      <input
                        type="radio"
                        name="speech-voice"
                        value={option.id}
                        checked={voice === option.id}
                        disabled={!option.downloaded}
                        onChange={() => setVoice(option.id)}
                        className="accent-accent"
                      />
                      {option.name}{' '}
                      <span className="text-xs text-text-muted">
                        {option.gender === 'female' ? 'Female' : 'Male'} · grade {option.grade}
                        {option.id === speech.defaultVoice ? ' · default' : ''}
                        {option.downloaded ? '' : ' · not downloaded'}
                      </span>
                    </label>
                    <button
                      type="button"
                      className={GHOST_BUTTON_CLASS}
                      disabled={!option.downloaded}
                      aria-label={previewing ? `Stop the ${option.name} preview` : `Preview ${option.name}`}
                      onClick={() => (previewing ? stopSpeaking(previewId(option.id)) : preview(option.id, option.name))}
                    >
                      {previewing ? <Pause className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" /> : <Volume2 className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />}
                      {previewing ? 'Stop' : 'Preview'}
                    </button>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection title="Pace">
        <SettingsRow label="Speed" hint="How fast the voice reads. 1× is its natural pace." htmlFor="speech-speed">
          <select
            id="speech-speed"
            value={speed}
            onChange={(event) => setSpeed(Number(event.target.value))}
            className={`${settingsFieldClass} w-28`}
          >
            {SPEEDS.map((option) => (
              <option key={option} value={option}>{`${option}×`}</option>
            ))}
          </select>
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
