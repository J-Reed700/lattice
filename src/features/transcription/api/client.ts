import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';
import type { Transcript, TranscriptionStatus } from '@/types/transcription';

export const transcriptionApi = {
  transcribeFile: async (path: string): Promise<ApiResult<Transcript>> =>
    apiCall<Wire.TranscriptDto>('transcribe_file', { path }),

  getTranscriptionStatus: async (): Promise<ApiResult<TranscriptionStatus>> =>
    apiCall<Wire.TranscriptionStatusDto>('get_transcription_status'),
};
