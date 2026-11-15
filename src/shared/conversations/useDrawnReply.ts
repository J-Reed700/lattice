import { useMemo } from 'react';

import type { ReplyPieceDto } from '@/lib/bindings';
import { drawReply, replyOf, type DrawnReply } from '@/utils/replyPieces';

/** How every surface that shows an assistant message draws it: from its pieces. */
export function useDrawnReply(
  message: { content: string; reply?: readonly ReplyPieceDto[] | null },
  enabled = true,
): DrawnReply | null {
  const { content, reply } = message;
  return useMemo(() => (enabled ? drawReply(replyOf({ content, reply })) : null), [enabled, content, reply]);
}
