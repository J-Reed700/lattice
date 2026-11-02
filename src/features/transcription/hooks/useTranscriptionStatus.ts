import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { describeFailure } from '@/lib/errorHandling';

export const TRANSCRIPTION_STATUS_QUERY_KEY = ['transcription', 'status'] as const;

/** Whether a Whisper model is downloaded, and which one a voice conversation should get. */
export function useTranscriptionStatusQuery() {
  const api = useApi();
  return useQuery({
    queryKey: TRANSCRIPTION_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getTranscriptionStatus();
      if (!result.ok) throw new Error(describeFailure(result));
      return result.data;
    },
    staleTime: 30_000,
  });
}
