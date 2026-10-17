/**
 * LearningWindowNotice
 *
 * The line a row holding the Learning role shows when the last learning work
 * refused needed a larger context window than this model has, with both
 * numbers, so the fix is made where the window is set.
 */

import { learningWindowShortfallText, useLearningWindowShortfall } from '@/features/model/hooks/useLearningWindowShortfall';
import type { ModelRef } from '@/lib/bindings';

interface Props {
  /** The row's model, as the Learning role names it. */
  model: ModelRef | null;
  /** The window saved for it in Settings → Models; null for Auto or a server's own. */
  savedWindow: number | null;
}

export function LearningWindowNotice({ model, savedWindow }: Props) {
  const text = learningWindowShortfallText(useLearningWindowShortfall(), model, savedWindow);
  if (!text) return null;
  return (
    <div role="status" className="mt-0.5 wrap-break-word text-xs text-[hsl(var(--warning-fg))]">
      {text}
    </div>
  );
}
