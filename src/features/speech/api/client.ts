import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

export const speechApi = {
  /**
   * Whether the read-aloud voices are downloaded, and what to download.
   *
   * @returns The voices, the default one and the download's size
   */
  getSpeechStatus: async (): Promise<ApiResult<Wire.SpeechStatusDto>> =>
    apiCall<Wire.SpeechStatusDto>('get_speech_status'),

  /**
   * Reads one piece of text aloud on this computer.
   *
   * @param text - A sentence or two of plain prose
   * @param voice - A voice id from `getSpeechStatus`
   * @param speed - 0.5 to 2, where 1 is the voice's natural pace
   * @returns 16-bit PCM and its sample rate
   */
  synthesizeSpeech: async (
    text: string,
    voice: string,
    speed: number,
  ): Promise<ApiResult<Wire.SpeechClipDto>> =>
    apiCall<Wire.SpeechClipDto>('synthesize_speech', { text, voice, speed }),
};
