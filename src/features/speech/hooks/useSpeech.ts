import { useCallback, useEffect, useSyncExternalStore } from 'react';

import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router';

import { useApi } from '@/contexts/ApiContext';
import { describeFailure } from '@/lib/errorHandling';
import { toast } from '@/stores/toastStore';

import { speechPlayer, type SpeechStatus } from '../model/speechPlayer';
import { speechChunks } from '../model/speechText';
import { useSpeechPrefsStore } from '../stores/speechPrefsStore';

export const SPEECH_STATUS_QUERY_KEY = ['speech', 'status'] as const;
export const READ_ALOUD_SETTINGS = '/settings?tab=read-aloud';

/** Whether the voices are downloaded, which voices there are, and the download's size. */
export function useSpeechStatusQuery() {
  const api = useApi();
  return useQuery({
    queryKey: SPEECH_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getSpeechStatus();
      if (!result.ok) throw new Error(describeFailure(result));
      return result.data;
    },
    staleTime: 30_000,
  });
}

/** What the app's one player is doing. */
export function useSpeechPlayback() {
  return useSyncExternalStore(speechPlayer.subscribe, speechPlayer.getSnapshot);
}

/** Stops read-aloud, or only `sourceId` when given. */
export function stopSpeaking(sourceId?: string) {
  speechPlayer.stop(sourceId);
}

/**
 * Stops read-aloud when `key` changes or the caller unmounts: the page,
 * conversation or course being read has left the screen.
 */
export function useStopSpeechOnChange(key: unknown) {
  useEffect(() => () => speechPlayer.stop(), [key]);
}

/** Speaks pieces of text as a source, in the chosen voice and pace. */
export function useSpeaker() {
  const api = useApi();
  const voice = useSpeechPrefsStore((state) => state.voice);
  const speed = useSpeechPrefsStore((state) => state.speed);
  return useCallback(
    async (sourceId: string, pieces: readonly string[], voiceOverride?: string) => {
      await speechPlayer.play(sourceId, pieces, async (text) => {
        const result = await api.synthesizeSpeech(text, voiceOverride ?? voice, speed);
        if (!result.ok) throw new Error(describeFailure(result));
        return result.data;
      });
      const { error } = speechPlayer.getSnapshot();
      if (error) toast.error("Couldn't read this aloud", { message: error });
    },
    [api, voice, speed],
  );
}

/** Play, pause and stop for one piece of markdown, known by `sourceId`. */
export function useListen(sourceId: string, markdown: string) {
  const snapshot = useSpeechPlayback();
  const speak = useSpeaker();
  const navigate = useNavigate();
  const { data: speech } = useSpeechStatusQuery();
  const status: SpeechStatus = snapshot.sourceId === sourceId ? snapshot.status : 'idle';

  const play = useCallback(() => {
    if (speech && !speech.modelReady) {
      toast.info('Download the read-aloud voices first', {
        message: `${speech.modelName} runs on this computer.`,
        action: { label: 'Open settings', onClick: () => void navigate(READ_ALOUD_SETTINGS) },
      });
      return;
    }
    void speak(sourceId, speechChunks(markdown));
  }, [speech, speak, sourceId, markdown, navigate]);

  return {
    status,
    play,
    pause: () => speechPlayer.pause(),
    resume: () => speechPlayer.resume(),
    stop: () => speechPlayer.stop(sourceId),
  };
}
