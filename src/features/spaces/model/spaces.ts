/** The space every chat and document belongs to until the user picks another. */
export const GENERAL_SPACE_ID = 'space_general';

/** A space's accent as lowercase `#rgb`/`#rrggbb`, or null when it is not a hex color. */
export const normalizeHexColor = (value: string | null | undefined): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  const isHex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate);
  return isHex ? candidate.toLowerCase() : null;
};

/** The tools a space turns on for its new turns, as `toolPreferencesJson` holds them. */
export interface SpaceToolPreferences {
  knowledgeBase: boolean;
  webSearch: boolean;
  deepResearchMode: boolean;
  enabledTools: string[];
}

export const buildSpaceToolPreferencesJson = ({
  knowledgeBase,
  webSearch,
  deepResearchMode,
}: {
  knowledgeBase: boolean;
  webSearch: boolean;
  deepResearchMode: boolean;
}): string =>
  JSON.stringify({
    knowledgeBase,
    webSearch,
    deepResearchMode,
    enabledTools: webSearch ? ['web_search', 'fetch_url_content'] : [],
  });

export const parseSpaceToolPreferences = (raw: string | null): SpaceToolPreferences => {
  if (!raw) {
    return { knowledgeBase: false, webSearch: false, deepResearchMode: false, enabledTools: [] };
  }

  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const knowledgeBase =
      typeof parsed.knowledgeBase === 'boolean'
        ? parsed.knowledgeBase
        : (typeof parsed.knowledge_base === 'boolean' ? parsed.knowledge_base : false);
    const webSearch =
      typeof parsed.webSearch === 'boolean'
        ? parsed.webSearch
        : (typeof parsed.web_search === 'boolean' ? parsed.web_search : false);
    const deepResearchMode =
      typeof parsed.deepResearchMode === 'boolean'
        ? parsed.deepResearchMode
        : (typeof parsed.deep_research_mode === 'boolean' ? parsed.deep_research_mode : false);
    const enabledToolsRaw =
      (Array.isArray(parsed.enabledTools) ? parsed.enabledTools : undefined) ??
      (Array.isArray(parsed.enabled_tools) ? parsed.enabled_tools : undefined);
    const enabledTools = enabledToolsRaw
      ? [...new Set(enabledToolsRaw.filter((tool): tool is string => typeof tool === 'string').map((tool) => tool.trim()).filter(Boolean))]
      : (webSearch ? ['web_search', 'fetch_url_content'] : []);

    return { knowledgeBase, webSearch, deepResearchMode, enabledTools };
  } catch {
    return { knowledgeBase: false, webSearch: false, deepResearchMode: false, enabledTools: [] };
  }
};

/**
 * A name none of `taken` uses, ignoring case. A blank request becomes
 * "`base` N" after the last one; a taken name gets " 2", " 3", … appended.
 */
export const uniqueName = (requested: string, taken: readonly string[], base: string): string => {
  const used = new Set(taken.map((name) => name.toLowerCase()));
  const name = requested.trim();
  if (!name) {
    let index = taken.length + 1;
    while (used.has(`${base} ${index}`.toLowerCase())) index += 1;
    return `${base} ${index}`;
  }
  if (!used.has(name.toLowerCase())) return name;
  let suffix = 2;
  while (used.has(`${name} ${suffix}`.toLowerCase())) suffix += 1;
  return `${name} ${suffix}`;
};
