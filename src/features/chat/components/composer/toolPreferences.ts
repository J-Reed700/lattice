import { type TurnMode } from '@/features/chat/components/ComposerControls';
import type { ToolPreferences } from '@/types';

import { WEB_TOOL_NAMES } from './toolNames';


const normalizeEnabledTools = (value: unknown): string[] | undefined => {
  if (!Array.isArray(value)) {
    return undefined;
  }

  return [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))];
};

export const setToolNames = (
  current: string[] | undefined,
  names: readonly string[],
  enabled: boolean
): string[] => {
  const set = new Set((current ?? []).map((name) => name.trim()).filter(Boolean));
  for (const name of names) {
    if (enabled) {
      set.add(name);
    } else {
      set.delete(name);
    }
  }
  return [...set];
};

export const defaultToolPreferences = (): ToolPreferences => ({
  knowledgeBase: false,
  webSearch: false,
  deepResearchMode: false,
  followupMode: false,
  turnMode: 'auto',
  enabledTools: [],
});

export const normalizeTurnMode = (value: unknown): TurnMode | null => {
  if (value === 'auto' || value === 'followup' || value === 'query') {
    return value;
  }
  return null;
};

const resolveTurnMode = (parsed: Record<string, unknown>): TurnMode => {
  const explicitTurnMode =
    normalizeTurnMode(parsed.turnMode) ?? normalizeTurnMode(parsed.turn_mode);
  if (explicitTurnMode) {
    return explicitTurnMode;
  }

  const followupMode =
    typeof parsed.followupMode === 'boolean'
      ? parsed.followupMode
      : (typeof parsed.followup_mode === 'boolean' ? parsed.followup_mode : false);

  return followupMode ? 'followup' : 'auto';
};

export const parseToolPreferences = (serialized: string | null | undefined): ToolPreferences => {
  const defaults = defaultToolPreferences();
  if (!serialized) {
    return defaults;
  }

  try {
    const parsed = JSON.parse(serialized) as Record<string, unknown>;
    const knowledgeBase =
      typeof parsed.knowledgeBase === 'boolean'
        ? parsed.knowledgeBase
        : (typeof parsed.knowledge_base === 'boolean'
          ? parsed.knowledge_base
          : defaults.knowledgeBase);
    const webSearch =
      typeof parsed.webSearch === 'boolean'
        ? parsed.webSearch
        : (typeof parsed.web_search === 'boolean' ? parsed.web_search : defaults.webSearch);
    const deepResearchMode =
      typeof parsed.deepResearchMode === 'boolean'
        ? parsed.deepResearchMode
        : (typeof parsed.deep_research_mode === 'boolean' ? parsed.deep_research_mode : false);
    const turnMode = resolveTurnMode(parsed);
    const followupMode = turnMode === 'followup';

    const enabledTools =
      normalizeEnabledTools(parsed.enabledTools) ?? normalizeEnabledTools(parsed.enabled_tools);
    if (enabledTools) {
      return {
        knowledgeBase,
        webSearch,
        deepResearchMode,
        followupMode,
        turnMode,
        enabledTools,
      };
    }

    return {
      knowledgeBase,
      webSearch,
      deepResearchMode,
      followupMode,
      turnMode,
      enabledTools: webSearch ? [...WEB_TOOL_NAMES] : [],
    };
  } catch {
    return defaults;
  }
};
