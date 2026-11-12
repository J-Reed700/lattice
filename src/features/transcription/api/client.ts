import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type { Transcript, TranscriptionStatus } from '@/types/transcription';

export const transcriptionApi = {
  /**
   * Transcribes an audio file on-device and returns timestamped segments.
   * Explicit re-run; normal ingest transcribes automatically.
   *
   * @param path - Absolute path to an mp3/wav/m4a/flac/ogg file
   * @returns Transcript with segments and rendered text
   */
  transcribeFile: async (path: string): Promise<ApiResult<Transcript>> =>
    apiCall('transcribe_file', { path }),

  /**
   * Hears a recording made in the app, on this computer.
   *
   * @param pcm - Mono 16-bit little-endian PCM at 16 kHz, base64
   * @returns What was said; empty when it was not speech
   */
  transcribeSpeech: async (pcm: string): Promise<ApiResult<Wire.TranscribedSpeechDto>> =>
    apiCall('transcribe_speech', { pcm }),

  /**
   * Reports whether a transcription model is downloaded.
   *
   * @returns TranscriptionStatus with the model name when ready
   */
  getTranscriptionStatus: async (): Promise<ApiResult<TranscriptionStatus>> =>
    apiCall('get_transcription_status'),
};
