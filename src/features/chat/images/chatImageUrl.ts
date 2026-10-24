/**
 * Where a stored chat picture is served from (design §7): the backend's
 * `lattice-image` scheme, which answers only the main window and serves only
 * bytes whose hash still matches.
 */

import { convertFileSrc } from '@tauri-apps/api/core';

import type { ChatImage } from '@/types/conversation';

/** The private scheme the backend registers for chat pictures. */
export const CHAT_IMAGE_PROTOCOL = 'lattice-image';

export function chatImageUrl(image: Pick<ChatImage, 'fileName'>): string {
  return convertFileSrc(image.fileName, CHAT_IMAGE_PROTOCOL);
}
