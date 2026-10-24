import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { LearningWindowShortfallDto, ModelRef } from '@/lib/bindings';
import { sameModelRef } from '@/utils/chatModelSelection';

export const LEARNING_WINDOW_SHORTFALL_QUERY_KEY = ['learning-window-shortfall'] as const;
export const SHORTFALL_RECHECK_MS = 10_000;

async function loadShortfall(): Promise<LearningWindowShortfallDto | null> {
  const result = await VaultAPI.lastLearningWindowShortfall();
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

export function useLearningWindowShortfall(): LearningWindowShortfallDto | null {
  const query = useQuery<LearningWindowShortfallDto | null>({
    queryKey: LEARNING_WINDOW_SHORTFALL_QUERY_KEY,
    queryFn: loadShortfall,
    refetchOnMount: 'always',
    refetchInterval: (query) => (query.state.data ? SHORTFALL_RECHECK_MS : false),
  });
  return query.data ?? null;
}

const formatTokens = (tokens: number) => tokens.toLocaleString('en-US');

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
