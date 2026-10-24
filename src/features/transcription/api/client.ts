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
    apiCall<Wire.TranscriptDto>('transcribe_file', { path }),

  /**
   * Reports whether a transcription model is downloaded.
   *
   * @returns TranscriptionStatus with the model name when ready
   */
  getTranscriptionStatus: async (): Promise<ApiResult<TranscriptionStatus>> =>
    apiCall<Wire.TranscriptionStatusDto>('get_transcription_status'),
};
