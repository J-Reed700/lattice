import { differenceInDays, formatDistanceToNowStrict, isToday, isYesterday, startOfDay } from 'date-fns';

export const formatRoleLabel = (role: string | null | undefined): string => {
  switch ((role ?? '').toLowerCase()) {
    case 'assistant':
      return 'Assistant';
    case 'user':
      return 'You';
    case 'system':
      return 'System';
    default:
      return role ? role.charAt(0).toUpperCase() + role.slice(1) : '';
  }
};
export const scrollToMessage = (messageId: string) => {
  let attempts = 0;
  const maxAttempts = 12;

  const tick = () => {
    const element = document.getElementById(`message-${messageId}`);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add('chat-message-highlighted');
      window.setTimeout(() => {
        element.classList.remove('chat-message-highlighted');
      }, 1500);
      return;
    }

    attempts += 1;
    if (attempts < maxAttempts) {
      window.setTimeout(tick, 120);
    }
  };

  window.setTimeout(tick, 80);
};

export interface SpaceToolPreferences {
  knowledgeBase: boolean;
  webSearch: boolean;
  deepResearchMode: boolean;
  enabledTools: string[];
}

export type SpaceKind = 'standard' | 'journal';

export const JOURNAL_SPACE_DEFAULT_ICON = '📓';
export const JOURNAL_SPACE_DEFAULT_ACCENT = '#aa503d'; // Default notebook cover accent; saved custom colors are preserved.

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

export const normalizeHexColor = (value: string | null | undefined): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  const isHex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate);
  return isHex ? candidate.toLowerCase() : null;
};

export const RELATIVE_UNIT_SUFFIX: Record<string, string> = {
  second: 's',
  minute: 'm',
  hour: 'h',
  day: 'd',
  week: 'w',
  month: 'mo',
  year: 'y',
};

/** "12 minutes" -> "12m", "2 days" -> "2d". Falls back to the long form. */
export const formatShortRelativeTime = (value: string | null | undefined): string => {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const raw = formatDistanceToNowStrict(date);
  const match = /^(\d+)\s+(second|minute|hour|day|week|month|year)s?$/.exec(raw);
  if (!match) return raw;
  return `${match[1]}${RELATIVE_UNIT_SUFFIX[match[2]] ?? ''}`;
};

export const getLocalDayKey = (value: Date): string => {
  const year = value.getFullYear();
  const month = `${value.getMonth() + 1}`.padStart(2, '0');
  const day = `${value.getDate()}`.padStart(2, '0');
  return `${year}-${month}-${day}`;
};

export const formatJournalGroupLabel = (dateValue: Date): string => {
  const today = new Date();
  const todayStart = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const targetStart = new Date(dateValue.getFullYear(), dateValue.getMonth(), dateValue.getDate());
  const diffDays = Math.round((todayStart.getTime() - targetStart.getTime()) / 86_400_000);

  if (diffDays === 0) return 'Today';
  if (diffDays === 1) return 'Yesterday';
  return targetStart.toLocaleDateString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    year: targetStart.getFullYear() === todayStart.getFullYear() ? undefined : 'numeric',
  });
};

export type TimeBucketKey = 'today' | 'yesterday' | 'last-7-days' | 'last-30-days' | 'older';

export const TIME_BUCKET_ORDER: readonly TimeBucketKey[] = [
  'today',
  'yesterday',
  'last-7-days',
  'last-30-days',
  'older',
];

export const TIME_BUCKET_LABELS: Record<TimeBucketKey, string> = {
  today: 'Today',
  yesterday: 'Yesterday',
  'last-7-days': 'Last 7 days',
  'last-30-days': 'Last 30 days',
  older: 'Older',
};

export const getTimeBucket = (updatedAt: Date, now: Date): TimeBucketKey => {
  if (isToday(updatedAt)) return 'today';
  if (isYesterday(updatedAt)) return 'yesterday';
  const diff = differenceInDays(startOfDay(now), startOfDay(updatedAt));
  if (diff < 7) return 'last-7-days';
  if (diff < 30) return 'last-30-days';
  return 'older';
};

export const SPACES_MODAL_LAYER_CLASSES = {
  root: 'z-[200]',
  backdrop: 'z-[210]',
  panel: 'z-[220]',
  content: 'relative z-[221]',
  section: 'relative z-[222]',
} as const;

export const isReferenceInboxEnabled = (): boolean => {
  try {
    const stored = localStorage.getItem('feature.referenceInbox.v1');
    if (stored === null) return true;
    return stored !== 'false';
  } catch {
    return true;
  }
};
