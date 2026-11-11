import { useCallback, useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router';

import { useApi } from '@/contexts/ApiContext';
import { READ_ALOUD_SETTINGS, useSpeaker } from '@/features/speech/hooks/useSpeech';
import { speechPlayer } from '@/features/speech/model/speechPlayer';
import { describeFailure } from '@/lib/errorHandling';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import type { ConversationsState, GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { ConversationMessage } from '@/types/conversation';

import { openMic } from '../model/micInput';
import { encodePcm } from '../model/pcm';
import { VoiceSession, type VoicePhase, type VoiceSessionDeps } from '../model/voiceSession';

const VOICE_SOURCE = 'voice-reply';

function replyBeingWritten(state: ConversationsState, conversationId: string | null): string {
  for (const message of state.optimisticMessages.values()) {
    if (message.role === 'assistant' && message.conversationId === conversationId) return message.content;
  }
  return '';
}

interface VoiceConversationOptions {
  conversationId: string | null;
  /** Sends what was said as a turn; `undefined` when the chat can't take one. */
  send: (_text: string) => Promise<GenerationOutcome | undefined>;
  /** Puts text in the composer. */
  fill: (_text: string) => void;
}

/**
 * The mic in a chat: pressing it starts a voice conversation in that chat, and
 * pressing it again, leaving the chat or closing it ends one.
 */
export function useVoiceConversation({ conversationId, send, fill }: VoiceConversationOptions) {
  const api = useApi();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const speak = useSpeaker();
  const cancelGeneration = useConversationsStore((state) => state.cancelGeneration);
  const reply = useConversationsStore((state) => replyBeingWritten(state, conversationId));
  const replying = useConversationsStore((state) =>
    conversationId !== null && state.inFlightGenerations.has(conversationId));

  const [phase, setPhase] = useState<VoicePhase>('off');
  const session = useRef<VoiceSession | null>(null);
  const waitingForChat = useRef<Array<() => void>>([]);
  const latest = useRef({ conversationId, send, fill, speak, replying });
  useEffect(() => {
    latest.current = { conversationId, send, fill, speak, replying };
  });

  useEffect(() => {
    session.current?.replyGrew(reply);
  }, [reply]);

  useEffect(() => {
    if (replying) return;
    for (const resolve of waitingForChat.current.splice(0)) resolve();
  }, [replying]);

  useEffect(() => () => {
    session.current?.stop();
    session.current = null;
  }, [conversationId]);

  const toggle = useCallback(() => {
    if (session.current) {
      session.current.stop();
      session.current = null;
      return;
    }
    const openSettings = {
      label: 'Open settings',
      onClick: () => void navigate(READ_ALOUD_SETTINGS),
    };
    const deps: VoiceSessionDeps = {
      ready: async () => {
        const [hearing, speaking] = await Promise.all([api.getTranscriptionStatus(), api.getSpeechStatus()]);
        if (!hearing.ok) throw new Error(describeFailure(hearing));
        if (!speaking.ok) throw new Error(describeFailure(speaking));
        if (!hearing.data.modelReady) {
          toast.info('Download Whisper first', { message: 'It hears you on this computer.', action: openSettings });
          return false;
        }
        if (!speaking.data.modelReady) {
          toast.info('Download the read-aloud voices first', {
            message: `${speaking.data.modelName} runs on this computer.`,
            action: openSettings,
          });
          return false;
        }
        return true;
      },
      openMic,
      transcribe: async (samples) => {
        const result = await api.transcribeSpeech(encodePcm(samples));
        if (!result.ok) throw new Error(describeFailure(result));
        return result.data.text;
      },
      send: async (text) => {
        const outcome = await latest.current.send(text);
        if (outcome !== undefined) return outcome;
        latest.current.fill(text);
        throw new Error('The chat can’t take a message yet, so what you said is in the message box.');
      },
      finalReply: () => {
        const id = latest.current.conversationId;
        const messages = id ? queryClient.getQueryData<ConversationMessage[]>(conversationKeys.messages(id)) : undefined;
        return [...(messages ?? [])].reverse().find((message) => message.role === 'assistant')?.content ?? '';
      },
      interruptTurn: () => void cancelGeneration(latest.current.conversationId),
      whenFree: () =>
        latest.current.replying
          ? new Promise((resolve) => {
              waitingForChat.current.push(resolve);
            })
          : Promise.resolve(),
      speak: async (pieces) => {
        await latest.current.speak(VOICE_SOURCE, pieces);
        await speechPlayer.whenIdle();
      },
      stopSpeaking: () => speechPlayer.stop(VOICE_SOURCE),
      isSpeaking: () => speechPlayer.getSnapshot().status !== 'idle',
      onPhase: (next) => {
        setPhase(next);
        if (next === 'off') session.current = null;
      },
      onFailure: (title, message) => toast.error(title, { message }),
    };
    session.current = new VoiceSession(deps);
    void session.current.start();
  }, [api, cancelGeneration, navigate, queryClient]);

  return { phase, toggle };
}
