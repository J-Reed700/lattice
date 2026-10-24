import { learningWindowShortfallText, useLearningWindowShortfall } from '@/features/model/hooks/useLearningWindowShortfall';
import type { ModelRef } from '@/lib/bindings';

interface Props {
  model: ModelRef | null;
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
