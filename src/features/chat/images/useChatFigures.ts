import { useMemo } from 'react';

import type { ChatFigure } from '@/components/TiptapEditor/extensions/chatImage';
import { messageImages } from '@/features/chat/controller/messageMetadata';
import type { DisplayMessage, SourceWithMetadata } from '@/types/conversation';
import { getSourceExternalUrl } from '@/utils/sourcePreview';

import { chatImageUrl } from './chatImageUrl';

/** One address however it was written: no fragment, no trailing slash, any case. */
function comparable(url: string): string {
  try {
    const parsed = new URL(url);
    parsed.hash = '';
    return `${parsed.host}${parsed.pathname.replace(/\/+$/, '')}${parsed.search}`.toLowerCase();
  } catch {
    return url.trim().toLowerCase();
  }
}

/**
 * The pictures a message shows, ready for the viewer: where each is served
 * from, and the citation number its source page has among the answer's
 * sources (none until the sources arrive at the end of the turn).
 */
export function useChatFigures(
  message: DisplayMessage,
  citationMap: ReadonlyMap<number, SourceWithMetadata>,
): ChatFigure[] {
  const images = useMemo(() => messageImages(message), [message]);
  return useMemo(() => {
    if (images.length === 0) return [];
    const numberByPage = new Map<string, number>();
    for (const [number, source] of citationMap) {
      const url = getSourceExternalUrl(source);
      if (url && !numberByPage.has(comparable(url))) numberByPage.set(comparable(url), number);
    }
    return images.map((image) => ({
      ref: image.ref,
      url: chatImageUrl(image),
      caption: image.caption,
      width: image.width,
      height: image.height,
      sourceUrl: image.sourceUrl,
      sourceTitle: image.sourceTitle,
      license: image.license,
      attribution: image.attribution,
      citationNumber: image.sourceUrl ? numberByPage.get(comparable(image.sourceUrl)) ?? null : null,
    }));
  }, [images, citationMap]);
}
