import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { LearningWindowShortfallDto, ModelRef } from '@/lib/bindings';
import { sameModelRef } from '@/utils/chatModelSelection';

/**
 * The last learning work refused because the Learning model's window was too
 * small. Read whenever Settings → Models mounts, which is where a refused
 * lesson sends the user.
 */
export const LEARNING_WINDOW_SHORTFALL_QUERY_KEY = ['learning-window-shortfall'] as const;

async function loadShortfall(): Promise<LearningWindowShortfallDto | null> {
  const result = await VaultAPI.lastLearningWindowShortfall();
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningWindowShortfall(): LearningWindowShortfallDto | null {
  const query = useQuery<LearningWindowShortfallDto | null>({
    queryKey: LEARNING_WINDOW_SHORTFALL_QUERY_KEY,
    queryFn: loadShortfall,
  });
  return query.data ?? null;
}

const formatTokens = (tokens: number) => tokens.toLocaleString('en-US');

/**
 * What to say under `model`, the Learning model, when the last learning work
 * refused needed a larger window than it has: "needs N tokens, this model has
 * M". `savedWindow` is the window saved for it in Settings → Models now (null
 * for Auto or a server's own window), so a window raised far enough since the
 * refusal says nothing; one moved to Auto says nothing until work runs again.
 */
export function learningWindowShortfallText(
  shortfall: LearningWindowShortfallDto | null,
  model: ModelRef | null | undefined,
  savedWindow: number | null,
): string | null {
  if (!shortfall || !sameModelRef(shortfall.model, model)) return null;
  const window = savedWindow ?? (shortfall.window_is_saved ? null : shortfall.window_tokens);
  if (window === null || window >= shortfall.needed_tokens) return null;
  return `Learning work needs a context window of ${formatTokens(shortfall.needed_tokens)} tokens; this model has ${formatTokens(window)}.`;
}
