import { convertFileSrc } from '@tauri-apps/api/core';

import type { ChatImage } from '@/types/conversation';

export const CHAT_IMAGE_PROTOCOL = 'lattice-image';

export function chatImageUrl(image: Pick<ChatImage, 'fileName'>): string {
  return convertFileSrc(image.fileName, CHAT_IMAGE_PROTOCOL);
}
