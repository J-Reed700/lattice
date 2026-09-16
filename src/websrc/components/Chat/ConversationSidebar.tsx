import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { differenceInDays, formatDistanceToNowStrict, isToday, isYesterday, startOfDay } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import {
  Plus,
  MessageSquare,
  Trash2,
  Loader2,
  Combine,
  AlertCircle,
  X,
  Star,
  Pin,
  PanelLeft,
  Archive,
  RotateCcw,
  ArrowUpRight,
  ChevronDown,
  ListChecks,
  Save,
  Settings2,
  FolderPlus,
  NotebookPen,
  Pencil,
  Check,
} from 'lucide-react';
import { createPortal } from 'react-dom';
import { useNavigate } from 'react-router';

import { buildSynthesisBlock } from '@/components/Journal/synthesisTargets';
import { IconButton } from '@/components/ui/IconButton';
import { SidebarHeader, SidebarSearch, SidebarTabs } from '@/components/ui/SidebarHeader';
import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';

import { useDebounce } from '../../hooks/useDebounce';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';
import {
  buildCapturedChatReferenceIndex,
  chatReferenceKey,
  type CapturedChatReference,
} from '../../utils/chatReferenceIndex';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

import type { ConversationMessageBookmarkDto } from '../../types';
import type { ConversationJournalDto } from '../../types/api/conversation';

const formatRoleLabel = (role: string | null | undefined): string => {
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

const scrollToMessage = (messageId: string) => {
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

interface SpaceToolPreferences {
  knowledgeBase: boolean;
  webSearch: boolean;
  deepResearchMode: boolean;
  enabledTools: string[];
}

type SpaceKind = 'standard' | 'journal';

const JOURNAL_SPACE_DEFAULT_ICON = '📓';
const JOURNAL_SPACE_DEFAULT_ACCENT = '#8b72ff'; // matches --accent (dark)

const buildSpaceToolPreferencesJson = ({
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

const parseSpaceToolPreferences = (raw: string | null): SpaceToolPreferences => {
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

const normalizeHexColor = (value: string | null | undefined): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  const isHex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate);
  return isHex ? candidate.toLowerCase() : null;
};

const RELATIVE_UNIT_SUFFIX: Record<string, string> = {
  second: 's',
  minute: 'm',
  hour: 'h',
  day: 'd',
  week: 'w',
  month: 'mo',
  year: 'y',
};

/** "12 minutes" -> "12m", "2 days" -> "2d". Falls back to the long form. */
const formatShortRelativeTime = (value: string | null | undefined): string => {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const raw = formatDistanceToNowStrict(date);
  const match = /^(\d+)\s+(second|minute|hour|day|week|month|year)s?$/.exec(raw);
  if (!match) return raw;
  return `${match[1]}${RELATIVE_UNIT_SUFFIX[match[2]] ?? ''}`;
};

const getLocalDayKey = (value: Date): string => {
  const year = value.getFullYear();
  const month = `${value.getMonth() + 1}`.padStart(2, '0');
  const day = `${value.getDate()}`.padStart(2, '0');
  return `${year}-${month}-${day}`;
};

const formatJournalGroupLabel = (dateValue: Date): string => {
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

type TimeBucketKey = 'today' | 'yesterday' | 'last-7-days' | 'last-30-days' | 'older';

const TIME_BUCKET_ORDER: readonly TimeBucketKey[] = [
  'today',
  'yesterday',
  'last-7-days',
  'last-30-days',
  'older',
];

const TIME_BUCKET_LABELS: Record<TimeBucketKey, string> = {
  today: 'Today',
  yesterday: 'Yesterday',
  'last-7-days': 'Last 7 days',
  'last-30-days': 'Last 30 days',
  older: 'Older',
};

const getTimeBucket = (updatedAt: Date, now: Date): TimeBucketKey => {
  if (isToday(updatedAt)) return 'today';
  if (isYesterday(updatedAt)) return 'yesterday';
  const diff = differenceInDays(startOfDay(now), startOfDay(updatedAt));
  if (diff < 7) return 'last-7-days';
  if (diff < 30) return 'last-30-days';
  return 'older';
};

const SPACES_MODAL_LAYER_CLASSES = {
  root: 'z-[200]',
  backdrop: 'z-[210]',
  panel: 'z-[220]',
  content: 'relative z-[221]',
  section: 'relative z-[222]',
} as const;

const isReferenceInboxEnabled = (): boolean => {
  try {
    const stored = localStorage.getItem('feature.referenceInbox.v1');
    if (stored === null) return true;
    return stored !== 'false';
  } catch {
    return true;
  }
};

interface ConversationSidebarProps {
  /** Hide the sidebar. Owned by ChatView, which also binds ⌘\. */
  onCollapse?: () => void;
}

export function ConversationSidebar({ onCollapse }: ConversationSidebarProps = {}) {
  const navigate = useNavigate();
  const prefersReducedMotion = useReducedMotion();
  const {
    spaces,
    selectedSpaceId,
    filterMode,
    searchQuery,
    conversations,
    activeConversationId,
    isLoading,
    error,
    loadConversations,
    setSelectedSpace,
    setFilterMode,
    setSearchQuery,
    createConversation,
    selectConversation,
    setConversationSaved,
    setConversationPinned,
    setConversationArchived,
    renameConversation,
    deleteConversation,
    loadSpaces,
    clearError,
  } = useConversationsStore();

  const [isCreating, setIsCreating] = useState(false);
  const [isSpacesOpen, setIsSpacesOpen] = useState(false);
  const sidebarRef = useRef<HTMLDivElement | null>(null);
  const [spacesPanelFrame, setSpacesPanelFrame] = useState<{
    left: number;
    top: number;
    height: number;
    width: number;
  } | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [renamingConversationId, setRenamingConversationId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [isSelectionMode, setIsSelectionMode] = useState(false);
  const [selectedConversationIds, setSelectedConversationIds] = useState<Set<string>>(new Set());
  const [bulkSpaceIdDraft, setBulkSpaceIdDraft] = useState('');
  const [isBulkMoving, setIsBulkMoving] = useState(false);
  const [journals, setJournals] = useState<ConversationJournalDto[]>([]);
  const [isLoadingJournals, setIsLoadingJournals] = useState(false);
  const [journalsLoadError, setJournalsLoadError] = useState<string | null>(null);
  const [newSpaceKindDraft, setNewSpaceKindDraft] = useState<SpaceKind>('standard');
  const [localQuery, setLocalQuery] = useState(searchQuery);
  const [snippetResults, setSnippetResults] = useState<ConversationMessageBookmarkDto[]>([]);
  const [isLoadingSnippets, setIsLoadingSnippets] = useState(false);
  const [selectedSnippetId, setSelectedSnippetId] = useState<string | null>(null);
  const [snippetRoleFilter, setSnippetRoleFilter] = useState<'all' | 'assistant' | 'user' | 'system'>('all');
  const [capturedReferenceIndex, setCapturedReferenceIndex] = useState<Map<string, CapturedChatReference>>(new Map());
  const [isLoadingCaptureIndex, setIsLoadingCaptureIndex] = useState(false);
  const [isSpaceEditorOpen, setIsSpaceEditorOpen] = useState(false);
  const [isCreateSpaceOpen, setIsCreateSpaceOpen] = useState(false);
  const [newSpaceNameDraft, setNewSpaceNameDraft] = useState('');
  const [isCreatingSpace, setIsCreatingSpace] = useState(false);
  const [isCreatingQuickJournal, setIsCreatingQuickJournal] = useState(false);
  const [isSavingSpace, setIsSavingSpace] = useState(false);
  const [isArchivingSpace, setIsArchivingSpace] = useState(false);
  const [isRestoringSpace, setIsRestoringSpace] = useState(false);
  const [spaceNameDraft, setSpaceNameDraft] = useState('');
  const [spaceDescriptionDraft, setSpaceDescriptionDraft] = useState('');
  const [spaceIconDraft, setSpaceIconDraft] = useState('');
  const [spaceAccentDraft, setSpaceAccentDraft] = useState('');
  const [spaceModelDraft, setSpaceModelDraft] = useState('');
  const [spacePromptDraft, setSpacePromptDraft] = useState('');
  const [spaceKbDefault, setSpaceKbDefault] = useState(false);
  const [spaceWebDefault, setSpaceWebDefault] = useState(false);
  const [spaceDeepResearchDefault, setSpaceDeepResearchDefault] = useState(false);
  const [ollamaDefaultModel, setOllamaDefaultModel] = useState('');
  const [referenceInboxEnabled] = useState(isReferenceInboxEnabled);
  const debouncedQuery = useDebounce(localQuery, 250);
  const { downloadedModelMap } = useDownloadedModels();

  const spaceNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) {
      map.set(space.id, space.name);
    }
    return map;
  }, [spaces]);
  const journalNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const journal of journals) {
      map.set(journal.id, journal.name);
    }
    return map;
  }, [journals]);
  const spaceAccentById = useMemo(() => {
    const map = new Map<string, string | null>();
    for (const space of spaces) {
      map.set(space.id, normalizeHexColor(space.accentColor));
    }
    return map;
  }, [spaces]);
  const spaceKindById = useMemo(() => {
    const map = new Map<string, SpaceKind>();
    for (const space of spaces) {
      map.set(space.id, 'standard');
    }
    for (const journal of journals) {
      map.set(journal.id, 'journal');
    }
    return map;
  }, [journals, spaces]);
  const selectedSpace = useMemo(() => {
    if (!selectedSpaceId) return null;
    return spaces.find((space) => space.id === selectedSpaceId) ?? null;
  }, [selectedSpaceId, spaces]);
  const selectedJournal = useMemo(() => {
    if (!selectedSpaceId) return null;
    return journals.find((journal) => journal.id === selectedSpaceId) ?? null;
  }, [journals, selectedSpaceId]);
  const isJournalScope = Boolean(
    selectedJournal
  );
  const scopeLabel = selectedSpace
    ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
    : selectedJournal
      ? `${selectedJournal.icon ? `${selectedJournal.icon} ` : ''}${selectedJournal.name} · Journal`
      : 'All spaces';

  const archivedSpaces = useMemo(
    () => spaces.filter((space) => space.isArchived),
    [spaces]
  );
  const activeSpacesOrdered = useMemo(
    () => spaces.filter((space) => !space.isArchived),
    [spaces]
  );
  const journalSpaces = useMemo(
    () => journals.filter((journal) => !journal.isArchived),
    [journals]
  );
  const standardSpaces = useMemo(
    () => activeSpacesOrdered,
    [activeSpacesOrdered]
  );
  const selectedConversationCount = selectedConversationIds.size;
  const hasConversations = conversations.length > 0;
  const areAllVisibleConversationsSelected = hasConversations
    && conversations.every((conversation) => selectedConversationIds.has(conversation.id));

  const availableSpaceModels = useMemo(() => {
    const options = new Set<string>();

    Array.from(downloadedModelMap.values())
      .filter((model) => model.model_type === 'language_model')
      .forEach((model) => {
        if (model.model_id?.trim()) {
          options.add(model.model_id.trim());
        }
      });

    if (ollamaDefaultModel.trim()) {
      options.add(ollamaDefaultModel.trim());
    }

    if (spaceModelDraft.trim()) {
      options.add(spaceModelDraft.trim());
    }

    return Array.from(options).sort((a, b) => a.localeCompare(b));
  }, [downloadedModelMap, ollamaDefaultModel, spaceModelDraft]);

  const loadJournals = useCallback(async (silent = true): Promise<boolean> => {
    setIsLoadingJournals(true);
    const result = await VaultAPI.listJournals();
    if (!result.ok) {
      setJournalsLoadError(result.error);
      if (!silent) {
        toast.error("Couldn't load journals", {
          message: result.error,
          duration: 4200,
        });
      }
      setIsLoadingJournals(false);
      return false;
    }
    setJournalsLoadError(null);
    setJournals(result.data);
    setIsLoadingJournals(false);
    return true;
  }, []);

  useEffect(() => {
    let cancelled = false;
    let retryTimer: number | null = null;

    const run = async () => {
      const ok = await loadJournals(true);
      if (!ok && !cancelled) {
        retryTimer = window.setTimeout(() => {
          void loadJournals(true);
        }, 1200);
      }
    };

    void run();

    const handleFocus = () => {
      void loadJournals(true);
    };

    const handleVisibility = () => {
      if (document.visibilityState === 'visible') {
        void loadJournals(true);
      }
    };

    window.addEventListener('focus', handleFocus);
    document.addEventListener('visibilitychange', handleVisibility);

    return () => {
      cancelled = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
      window.removeEventListener('focus', handleFocus);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [loadJournals]);

  useEffect(() => {
    if (!isSelectionMode) {
      return;
    }
    void loadJournals(true);
  }, [isSelectionMode, loadJournals]);

  useEffect(() => {
    if (!isSpacesOpen) {
      return;
    }
    void loadJournals(true);
  }, [isSpacesOpen, loadJournals]);

  const journalConversationGroups = useMemo(() => {
    if (!isJournalScope) {
      const now = new Date();
      const buckets = new Map<TimeBucketKey, typeof conversations>();
      for (const conversation of conversations) {
        const updatedAt = new Date(conversation.updatedAt);
        const baseDate = Number.isNaN(updatedAt.getTime()) ? now : updatedAt;
        const bucket = getTimeBucket(baseDate, now);
        const existing = buckets.get(bucket);
        if (existing) {
          existing.push(conversation);
        } else {
          buckets.set(bucket, [conversation]);
        }
      }

      return TIME_BUCKET_ORDER
        .filter((key) => buckets.has(key))
        .map((key) => {
          const items = [...(buckets.get(key) ?? [])].sort((a, b) => {
            const aTime = new Date(a.updatedAt).getTime();
            const bTime = new Date(b.updatedAt).getTime();
            return (Number.isNaN(bTime) ? 0 : bTime) - (Number.isNaN(aTime) ? 0 : aTime);
          });
          return {
            key,
            label: TIME_BUCKET_LABELS[key],
            items,
          };
        });
    }

    const groups = new Map<string, { label: string; dayValue: Date; items: typeof conversations }>();
    for (const conversation of conversations) {
      const updatedAt = new Date(conversation.updatedAt);
      const baseDate = Number.isNaN(updatedAt.getTime()) ? new Date() : updatedAt;
      const dayValue = new Date(baseDate.getFullYear(), baseDate.getMonth(), baseDate.getDate());
      const key = getLocalDayKey(dayValue);
      const existing = groups.get(key);
      if (!existing) {
        groups.set(key, {
          label: formatJournalGroupLabel(dayValue),
          dayValue,
          items: [conversation],
        });
      } else {
        existing.items.push(conversation);
      }
    }

    return Array.from(groups.entries())
      .sort((a, b) => b[1].dayValue.getTime() - a[1].dayValue.getTime())
      .map(([key, group]) => ({
        key,
        label: group.label,
        items: group.items,
      }));
  }, [conversations, isJournalScope]);

  useEffect(() => {
    setLocalQuery(searchQuery);
  }, [searchQuery]);

  useEffect(() => {
    if (!selectedSpaceId) return;
    if (!selectedJournal) return;
    setSelectedSpace(null);
    void loadConversations({ spaceId: null });
  }, [loadConversations, selectedJournal, selectedSpaceId, setSelectedSpace]);

  useEffect(() => {
    if (!selectedSpace) {
      setSpaceNameDraft('');
      setSpaceDescriptionDraft('');
      setSpaceIconDraft('');
      setSpaceAccentDraft('');
      setSpaceModelDraft('');
      setSpacePromptDraft('');
      setSpaceKbDefault(false);
      setSpaceWebDefault(false);
      setSpaceDeepResearchDefault(false);
      return;
    }

    const defaults = parseSpaceToolPreferences(selectedSpace.toolPreferencesJson);
    setSpaceNameDraft(selectedSpace.name);
    setSpaceDescriptionDraft(selectedSpace.description ?? '');
    setSpaceIconDraft(selectedSpace.icon ?? '');
    setSpaceAccentDraft(selectedSpace.accentColor ?? '');
    setSpaceModelDraft(selectedSpace.defaultModelName ?? '');
    setSpacePromptDraft(selectedSpace.spacePrompt ?? '');
    setSpaceKbDefault(defaults.knowledgeBase);
    setSpaceWebDefault(defaults.webSearch);
    setSpaceDeepResearchDefault(defaults.deepResearchMode);
  }, [selectedSpace]);

  useEffect(() => {
    if (debouncedQuery === searchQuery) return;
    setSearchQuery(debouncedQuery);
    void loadConversations({ searchQuery: debouncedQuery });
  }, [debouncedQuery, loadConversations, searchQuery, setSearchQuery]);

  useEffect(() => {
    if (!isSelectionMode) {
      setSelectedConversationIds(new Set());
      setBulkSpaceIdDraft('');
      return;
    }

    setRenamingConversationId(null);
    setRenameDraft('');
  }, [isSelectionMode]);

  useEffect(() => {
    if (!isSelectionMode) return;
    const visibleConversationIds = new Set(conversations.map((conversation) => conversation.id));
    setSelectedConversationIds((prev) => {
      const next = new Set<string>();
      prev.forEach((conversationId) => {
        if (visibleConversationIds.has(conversationId)) {
          next.add(conversationId);
        }
      });
      return next;
    });
  }, [conversations, isSelectionMode]);

  useEffect(() => {
    if (filterMode !== 'snippets') return;
    setIsSelectionMode(false);
  }, [filterMode]);

  useEffect(() => {
    if (!isSelectionMode) return;
    if (bulkSpaceIdDraft) {
      const existing = journalSpaces.some((space) => space.id === bulkSpaceIdDraft);
      if (existing) return;
    }

    const selectedIsJournal = selectedSpaceId
      ? journalSpaces.some((space) => space.id === selectedSpaceId)
      : false;
    const fallbackSpaceId = selectedIsJournal
      ? selectedSpaceId ?? ''
      : (journalSpaces[0]?.id ?? '');
    setBulkSpaceIdDraft(fallbackSpaceId);
  }, [
    bulkSpaceIdDraft,
    isSelectionMode,
    journalSpaces,
    selectedSpaceId,
  ]);

  useEffect(() => {
    let cancelled = false;
    const loadOllamaModel = async () => {
      const result = await VaultAPI.getSettings();
      if (!result.ok || cancelled) return;

      const modelName = result.data.llm?.model?.trim?.() ?? '';
      if (modelName) {
        setOllamaDefaultModel(modelName);
      }
    };

    void loadOllamaModel();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (filterMode !== 'snippets') return;

    let cancelled = false;
    const run = async () => {
      setIsLoadingSnippets(true);
      const result = await VaultAPI.listMessageBookmarks({
        query: debouncedQuery.trim() ? debouncedQuery.trim() : undefined,
        limit: 60,
        offset: 0,
      });

      if (cancelled) return;
      if (!result.ok) {
        setIsLoadingSnippets(false);
        return;
      }

      const raw = Array.isArray(result.data) ? [] : result.data.bookmarks;
      const filtered = selectedSpaceId
        ? raw.filter((bookmark) => bookmark.spaceId === selectedSpaceId)
        : raw;
      setSnippetResults(filtered);
      setIsLoadingSnippets(false);
    };

    void run();

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, filterMode, selectedSpaceId]);

  useEffect(() => {
    if (!referenceInboxEnabled || filterMode !== 'snippets') return;

    let cancelled = false;
    const run = async () => {
      setIsLoadingCaptureIndex(true);
      const result = await VaultAPI.listWorkspaceNotes();
      if (cancelled) return;

      if (!result.ok) {
        setIsLoadingCaptureIndex(false);
        return;
      }

      setCapturedReferenceIndex(buildCapturedChatReferenceIndex(result.data.notes));
      setIsLoadingCaptureIndex(false);
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [filterMode, referenceInboxEnabled]);

  const roleFilteredSnippets = useMemo(() => {
    if (snippetRoleFilter === 'all') return snippetResults;
    return snippetResults.filter((snippet) => snippet.messageRole === snippetRoleFilter);
  }, [snippetResults, snippetRoleFilter]);

  const filteredSnippets = roleFilteredSnippets;

  const snippetCaptureStats = useMemo(() => {
    const total = roleFilteredSnippets.length;
    if (total === 0) {
      return { total: 0, captured: 0, pending: 0 };
    }
    const captured = roleFilteredSnippets.reduce((count, snippet) => {
      const key = chatReferenceKey(snippet.conversationId, snippet.messageId);
      return capturedReferenceIndex.has(key) ? count + 1 : count;
    }, 0);
    return {
      total,
      captured,
      pending: Math.max(0, total - captured),
    };
  }, [capturedReferenceIndex, roleFilteredSnippets]);

  useEffect(() => {
    if (filterMode !== 'snippets') return;
    if (filteredSnippets.length === 0) {
      setSelectedSnippetId(null);
      return;
    }

    const resolved = filteredSnippets.find((snippet) => snippet.id === selectedSnippetId) ?? filteredSnippets[0];
    setSelectedSnippetId(resolved.id);
  }, [filterMode, filteredSnippets, selectedSnippetId]);

  const handleNewConversation = async () => {
    setIsCreating(true);
    try {
      const title = createDefaultConversationTitle();
      await createConversation(title);
    } finally {
      setIsCreating(false);
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();

    const target = conversations.find((conversation) => conversation.id === id);
    if (!target) {
      return;
    }

    setDeletingId(id);
    try {
      await deleteConversation(id);
    } finally {
      setDeletingId(null);
    }
  };

  const beginRenameConversation = (conversationId: string, title: string) => {
    setRenamingConversationId(conversationId);
    setRenameDraft(title);
  };

  const cancelRenameConversation = () => {
    setRenamingConversationId(null);
    setRenameDraft('');
  };

  const [synthesizingConversationId, setSynthesizingConversationId] = useState<string | null>(
    null,
  );

  /**
   * Writes a synthesis of one conversation onto the journal. `quickCapture`
   * lands on today's page, and we open the Journal on that page by id so the
   * user arrives at what was just written rather than wherever they left off.
   */
  const synthesizeConversationToJournal = useCallback(
    async (conversationId: string, conversationTitle: string) => {
      setSynthesizingConversationId(conversationId);
      try {
        const result = await VaultAPI.synthesizeJournalEntries({
          conversationIds: [conversationId],
          scope: 'conversation',
          maxEntries: 1,
        });
        if (!result.ok) {
          toast.error('Synthesis failed', { message: result.error });
          return;
        }
        const block = buildSynthesisBlock({
          heading: conversationTitle,
          entryCount: result.data.entryCount,
          synthesis: result.data.synthesis,
          citations: result.data.citations,
        });
        const capture = await VaultAPI.quickCapture(block);
        if (!capture.ok) {
          toast.error('Could not write to the journal', { message: capture.error });
          return;
        }
        // We navigate to the page itself, so an "Open" action on the toast
        // would be a control that changes nothing.
        toast.success(`Synthesis saved to "${capture.data.noteTitle}"`);
        navigate(`/journals?${new URLSearchParams({ noteId: capture.data.noteId }).toString()}`);
      } finally {
        setSynthesizingConversationId(null);
      }
    },
    [navigate],
  );

  const synthesizePaletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'chat.synthesizeToJournal',
        label: 'Synthesize this conversation to Journal',
        group: 'Journal',
        icon: Combine,
        enabled: Boolean(activeConversationId),
        run: () => {
          if (!activeConversationId) return;
          const conversation = conversations.find((c) => c.id === activeConversationId);
          void synthesizeConversationToJournal(
            activeConversationId,
            conversation?.title ?? 'Conversation',
          );
        },
      },
    ],
    [activeConversationId, conversations, synthesizeConversationToJournal],
  );
  useRegisterPaletteCommands(synthesizePaletteCommands);

  const saveConversationRename = async (
    conversationId: string,
    currentTitle: string
  ) => {
    const nextTitle = renameDraft.trim();
    if (!nextTitle) {
      toast.warning('Title required', {
        message: 'Conversation title cannot be empty.',
        duration: 2800,
      });
      return;
    }

    if (nextTitle === currentTitle.trim()) {
      cancelRenameConversation();
      return;
    }

    const didRename = await renameConversation(conversationId, nextTitle);
    if (didRename) {
      cancelRenameConversation();
    }
  };

  const toggleConversationSelection = (conversationId: string) => {
    setSelectedConversationIds((prev) => {
      const next = new Set(prev);
      if (next.has(conversationId)) {
        next.delete(conversationId);
      } else {
        next.add(conversationId);
      }
      return next;
    });
  };

  const toggleSelectAllVisibleConversations = () => {
    if (!hasConversations) return;

    if (areAllVisibleConversationsSelected) {
      setSelectedConversationIds(new Set());
      return;
    }

    setSelectedConversationIds(new Set(conversations.map((conversation) => conversation.id)));
  };

  const addSelectedConversationsToJournal = async () => {
    const targetSpaceId = bulkSpaceIdDraft.trim();
    const selectedIds = Array.from(selectedConversationIds);

    if (selectedIds.length === 0) {
      toast.warning('No conversations selected');
      return;
    }
    if (!targetSpaceId) {
      toast.warning('Choose a destination journal');
      return;
    }
    if (!journalSpaces.some((journal) => journal.id === targetSpaceId)) {
      toast.warning('Choose a destination journal');
      return;
    }

    const destinationName =
      journalNameById.get(targetSpaceId) ?? spaceNameById.get(targetSpaceId) ?? 'selected journal';
    setIsBulkMoving(true);
    try {
      const moveResults = await Promise.all(
        selectedIds.map(async (conversationId) => ({
          conversationId,
          result: await VaultAPI.addConversationToJournal({
            journalSpaceId: targetSpaceId,
            conversationId,
          }),
        }))
      );

      const failedMoves = moveResults.filter(({ result }) => !result.ok);
      const addedCount = selectedIds.length - failedMoves.length;

      if (failedMoves.length === 0) {
        toast.success('Added to journal', {
          message: `${addedCount} added to ${destinationName}. Conversations stay in their current space.`,
          duration: 3200,
        });
        setSelectedConversationIds(new Set());
        setIsSelectionMode(false);
        return;
      }

      setSelectedConversationIds(
        new Set(failedMoves.map(({ conversationId }) => conversationId))
      );
      toast.warning('Some conversations were not added', {
        message: `${addedCount}/${selectedIds.length} added to ${destinationName}.`,
        duration: 4500,
      });
    } finally {
      setIsBulkMoving(false);
    }
  };

  const createQuickJournal = async () => {
    if (isCreatingQuickJournal) return;

    setIsCreatingQuickJournal(true);
    try {
      const existingNames = new Set(journals.map((journal) => journal.name.toLowerCase()));
      let idx = journals.length + 1;
      let name = `Journal ${idx}`;
      while (existingNames.has(name.toLowerCase())) {
        idx += 1;
        name = `Journal ${idx}`;
      }

      const result = await VaultAPI.createJournal({
        name,
        description: null,
        icon: JOURNAL_SPACE_DEFAULT_ICON,
        accentColor: JOURNAL_SPACE_DEFAULT_ACCENT,
        spacePrompt: null,
        defaultModelName: null,
        toolPreferencesJson: null,
      });
      if (!result.ok) {
        toast.error("Couldn't create journal", {
          message: result.error,
          duration: 4200,
        });
        return;
      }

      setJournals((prev) => [result.data, ...prev]);
      setJournalsLoadError(null);
      setBulkSpaceIdDraft(result.data.id);
      toast.success('Journal created', {
        message: `${result.data.name} is ready.`,
        duration: 2600,
      });
    } finally {
      setIsCreatingQuickJournal(false);
    }
  };

  const handleSpaceSelect = async (spaceId: string | null) => {
    setSelectedSpace(spaceId);
    await loadConversations({ spaceId });
  };

  const handleFilterSelect = async (
    mode: 'all' | 'saved' | 'bookmarked' | 'pinned' | 'archived' | 'snippets'
  ) => {
    setFilterMode(mode);
    await loadConversations({ filterMode: mode });
  };

  type FilterId = 'all' | 'saved' | 'bookmarked' | 'pinned' | 'archived' | 'snippets';

  const filterOptions: ReadonlyArray<{ id: FilterId; label: string }> = [
    { id: 'all', label: 'All' },
    { id: 'saved', label: 'Starred' },
    { id: 'pinned', label: 'Pinned' },
    { id: 'archived', label: 'Archived' },
    { id: 'snippets', label: 'Referenced' },
  ];

  const openSnippet = async (bookmark: ConversationMessageBookmarkDto) => {
    await selectConversation(bookmark.conversationId);
    scrollToMessage(bookmark.messageId);
  };

  const openSelectedJournalNotebook = () => {
    const targetJournalId = (
      bulkSpaceIdDraft.trim()
      && journalSpaces.some((journal) => journal.id === bulkSpaceIdDraft.trim())
    )
      ? bulkSpaceIdDraft.trim()
      : (journalSpaces[0]?.id ?? null);
    if (!targetJournalId) {
      return;
    }

    const params = new URLSearchParams({
      journalSpaceId: targetJournalId,
      panel: 'entries',
    });
    if (activeConversationId) {
      params.set('entryId', activeConversationId);
    }
    navigate(`/journals?${params.toString()}`);
  };

  const getCapturedSnippetReference = (
    bookmark: Pick<ConversationMessageBookmarkDto, 'conversationId' | 'messageId'>
  ): CapturedChatReference | null => {
    const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
    return capturedReferenceIndex.get(key) ?? null;
  };

  const createSpace = async () => {
    setIsCreatingSpace(true);
    try {
      const requestedName = newSpaceNameDraft.trim();
      const existingNames = new Set(
        (newSpaceKindDraft === 'journal' ? journals : spaces).map((item) =>
          item.name.toLowerCase()
        )
      );
      const defaultBaseName = newSpaceKindDraft === 'journal' ? 'Journal' : 'Space';

      let name = requestedName;
      if (!name) {
        let idx = (newSpaceKindDraft === 'journal' ? journals.length : spaces.length) + 1;
        name = `${defaultBaseName} ${idx}`;
        while (existingNames.has(name.toLowerCase())) {
          idx += 1;
          name = `${defaultBaseName} ${idx}`;
        }
      } else if (existingNames.has(name.toLowerCase())) {
        let suffix = 2;
        let candidate = `${name} ${suffix}`;
        while (existingNames.has(candidate.toLowerCase())) {
          suffix += 1;
          candidate = `${name} ${suffix}`;
        }
        name = candidate;
      }

      if (newSpaceKindDraft === 'journal') {
        const result = await VaultAPI.createJournal({
          name,
          description: null,
          icon: JOURNAL_SPACE_DEFAULT_ICON,
          accentColor: JOURNAL_SPACE_DEFAULT_ACCENT,
          spacePrompt: null,
          defaultModelName: null,
          toolPreferencesJson: null,
        });
        if (!result.ok) {
          return;
        }

        await loadJournals();
        setSelectedSpace(null);
        await loadConversations({ spaceId: null });
        navigate(`/journals?journalSpaceId=${encodeURIComponent(result.data.id)}&panel=entries`);
      } else {
        const result = await VaultAPI.createConversationSpace({
          name,
          description: null,
          icon: null,
          accentColor: null,
          spacePrompt: null,
          defaultModelName: null,
          toolPreferencesJson: buildSpaceToolPreferencesJson({
            knowledgeBase: false,
            webSearch: false,
            deepResearchMode: false,
          }),
        });
        if (!result.ok) {
          return;
        }

        await loadSpaces();
        setSelectedSpace(result.data.id);
        await loadConversations({ spaceId: result.data.id });
        setIsSpaceEditorOpen(true);
      }

      setIsCreateSpaceOpen(false);
      setNewSpaceNameDraft('');
      setNewSpaceKindDraft('standard');
    } finally {
      setIsCreatingSpace(false);
    }
  };

  const saveSpaceEnvironment = async () => {
    if (!selectedSpace) return;

    setIsSavingSpace(true);
    try {
      const payload = {
        spaceId: selectedSpace.id,
        name: spaceNameDraft.trim() || selectedSpace.name,
        description: spaceDescriptionDraft.trim() ? spaceDescriptionDraft.trim() : null,
        icon: spaceIconDraft.trim() ? spaceIconDraft.trim() : null,
        accentColor: normalizeHexColor(spaceAccentDraft),
        defaultModelName: spaceModelDraft.trim() ? spaceModelDraft.trim() : null,
        spacePrompt: spacePromptDraft.trim() ? spacePromptDraft.trim() : null,
        toolPreferencesJson: buildSpaceToolPreferencesJson({
          knowledgeBase: spaceKbDefault,
          webSearch: spaceWebDefault,
          deepResearchMode: spaceDeepResearchDefault,
        }),
      };

      const result = await VaultAPI.updateConversationSpace(payload);
      if (!result.ok) {
        return;
      }

      await loadSpaces();
    } finally {
      setIsSavingSpace(false);
    }
  };

  const setSelectedSpaceArchived = async (archived: boolean) => {
    if (!selectedSpace || selectedSpace.id === 'space_general') return;

    setIsArchivingSpace(true);
    try {
      const result = await VaultAPI.archiveConversationSpace({
        spaceId: selectedSpace.id,
        archived,
      });
      if (!result.ok) {
        return;
      }

      if (archived) {
        setIsSpaceEditorOpen(false);
        setSelectedSpace(null);
        await Promise.all([
          loadSpaces(),
          loadConversations({ spaceId: null }),
        ]);
      } else {
        await loadSpaces();
      }
    } finally {
      setIsArchivingSpace(false);
    }
  };

  const restoreArchivedSpace = async (spaceId: string) => {
    setIsRestoringSpace(true);
    try {
      const result = await VaultAPI.archiveConversationSpace({
        spaceId,
        archived: false,
      });
      if (!result.ok) {
        return;
      }
      await loadSpaces();
    } finally {
      setIsRestoringSpace(false);
    }
  };

  const toggleSpaceDeepResearchDefault = () => {
    const next = !spaceDeepResearchDefault;
    setSpaceDeepResearchDefault(next);
    if (next) {
      toast.warning('Space Deep Research default enabled', {
        message:
          'New turns in this space may take significantly longer because deep research performs recursive retrieval.',
        duration: 5000,
      });
    }
  };

  const updateSpacesPanelFrame = useCallback(() => {
    const sidebarElement = sidebarRef.current;
    if (!sidebarElement) return;

    const rect = sidebarElement.getBoundingClientRect();
    const availableWidth = Math.max(220, Math.floor(window.innerWidth - rect.right - 12));
    const width = Math.min(360, availableWidth);

    setSpacesPanelFrame({
      left: Math.round(rect.right),
      top: Math.round(rect.top),
      height: Math.round(rect.height),
      width,
    });
  }, []);

  useEffect(() => {
    if (!isSpacesOpen) {
      setSpacesPanelFrame(null);
      return;
    }

    updateSpacesPanelFrame();
    const handleLayoutChange = () => updateSpacesPanelFrame();
    window.addEventListener('resize', handleLayoutChange);
    window.addEventListener('scroll', handleLayoutChange, true);

    return () => {
      window.removeEventListener('resize', handleLayoutChange);
      window.removeEventListener('scroll', handleLayoutChange, true);
    };
  }, [isSpacesOpen, updateSpacesPanelFrame]);

  return (
    <div
      ref={sidebarRef}
      className={`relative flex h-full w-[280px] max-w-full shrink-0 flex-col overflow-hidden border-r border-border-subtle bg-surface ${
        isSpacesOpen ? SPACES_MODAL_LAYER_CLASSES.root : ''
      }`}
    >
      <SidebarHeader
        title={isJournalScope && selectedJournal ? selectedJournal.name : 'Chat'}
        actions={
          <>
            <IconButton
              label={isJournalScope ? 'New entry' : 'New conversation'}
              shortcut="⌘N"
              onClick={handleAsyncEvent(handleNewConversation)}
              disabled={isCreating}
            >
              {isCreating ? <Loader2 className="animate-spin" /> : <Plus />}
            </IconButton>
            {filterMode !== 'snippets' && (
              <IconButton
                label="Select"
                active={isSelectionMode}
                onClick={() => {
                  const nextSelectionMode = !isSelectionMode;
                  setIsSelectionMode(nextSelectionMode);
                  if (nextSelectionMode) {
                    void loadJournals(false);
                  }
                }}
              >
                <ListChecks />
              </IconButton>
            )}
            <IconButton label="Hide sidebar" shortcut="⌘\" onClick={onCollapse}>
              <PanelLeft />
            </IconButton>
          </>
        }
      />

      {/* Scope — one line, opens the Spaces panel */}
      <div className="flex shrink-0 items-center gap-1 border-b border-border-subtle px-4 py-2">
        <button
          type="button"
          onClick={() => setIsSpacesOpen(true)}
          aria-label="Change scope"
          className="flex min-w-0 flex-1 items-center justify-between gap-2 rounded-sm px-1 py-0.5 text-sm text-text-primary transition-colors duration-fast hover:bg-surface-raised"
        >
          <span className="truncate">{scopeLabel}</span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" aria-hidden="true" />
        </button>
        {isJournalScope && selectedJournal && (
          <IconButton label="Open notebook" onClick={openSelectedJournalNotebook}>
            <NotebookPen />
          </IconButton>
        )}
      </div>

      <div className="shrink-0 space-y-2.5 border-b border-border-subtle px-4 py-2.5">
        <SidebarSearch
          value={localQuery}
          onChange={setLocalQuery}
          placeholder={isJournalScope ? 'Search entries' : 'Search conversations'}
        />
        <SidebarTabs
          value={filterMode as FilterId}
          onChange={handleAsyncEvent((id: FilterId) => handleFilterSelect(id))}
          options={filterOptions}
          className="justify-between gap-2"
        />
      </div>

      {isSelectionMode && filterMode !== 'snippets' && (
        <div className="shrink-0 space-y-1.5 border-b border-border-subtle px-4 py-2">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-text-muted">{selectedConversationCount} selected</span>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={toggleSelectAllVisibleConversations}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                {areAllVisibleConversationsSelected ? 'Clear' : 'Select all'}
              </button>
              <button
                type="button"
                onClick={() => setIsSelectionMode(false)}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Done
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <select
              value={bulkSpaceIdDraft}
              onChange={(e) => setBulkSpaceIdDraft(e.target.value)}
              disabled={isLoadingJournals}
              aria-label="Move to journal"
              className="h-7 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-2 text-xs text-text-primary outline-none transition-colors duration-fast focus:border-accent"
            >
              {isLoadingJournals ? (
                <option value="" disabled>
                  Loading…
                </option>
              ) : journalsLoadError ? (
                <option value="" disabled>
                  Couldn't load journals
                </option>
              ) : journalSpaces.length === 0 ? (
                <option value="" disabled>
                  No journals yet
                </option>
              ) : (
                <>
                  <option value="" disabled>
                    Choose a journal
                  </option>
                  {journalSpaces.map((space) => (
                    <option key={space.id} value={space.id}>
                      {space.name}
                    </option>
                  ))}
                </>
              )}
            </select>
            <button
              type="button"
              onClick={handleAsyncEvent(addSelectedConversationsToJournal)}
              disabled={
                selectedConversationCount === 0
                || !bulkSpaceIdDraft
                || isBulkMoving
                || Boolean(journalsLoadError)
                || journalSpaces.length === 0
              }
              className="inline-flex shrink-0 items-center gap-1 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isBulkMoving ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
              Add
            </button>
          </div>

          {journalsLoadError ? (
            <p className="text-xs text-[hsl(var(--danger-fg))]">
              Couldn't load journals. {journalsLoadError}
            </p>
          ) : journalSpaces.length === 0 ? (
            <div className="flex items-center gap-3 text-xs">
              <button
                type="button"
                onClick={handleAsyncEvent(createQuickJournal)}
                disabled={isCreatingQuickJournal}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-60"
              >
                New journal
              </button>
              <button
                type="button"
                onClick={() => navigate('/journals')}
                className="text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Open Journal
              </button>
            </div>
          ) : null}
        </div>
      )}

      {error && (
        <div className="mx-4 mt-4 flex items-center justify-between gap-3 rounded-sm border border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] px-3 py-2">
          <div className="flex items-center gap-2">
            <AlertCircle className="w-4 h-4 text-[hsl(var(--danger-fg))]" />
            <p className="text-xs text-[hsl(var(--danger-fg))]">{error}</p>
          </div>
          <button
            onClick={clearError}
            className="text-[hsl(var(--danger-fg))] opacity-70 transition-opacity hover:opacity-100"
            aria-label="Dismiss error"
            title="Dismiss"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {filterMode === 'snippets' && (
          <div className="border-b border-border-subtle pb-2">
            <div className="flex items-center gap-3 px-4 pb-2 pt-3 text-xs">
              {([
                ['all', 'All'],
                ['assistant', 'Assistant'],
                ['user', 'You'],
                ['system', 'System'],
              ] as const).map(([value, label]) => (
                <button
                  key={value}
                  onClick={() => setSnippetRoleFilter(value)}
                  className={`transition-colors duration-fast ${
                    snippetRoleFilter === value
                      ? 'text-text-primary'
                      : 'text-text-tertiary hover:text-text-primary'
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>

            <div className="flex items-center justify-between gap-2 px-4 pb-2 text-xs">
              {referenceInboxEnabled ? (
                <span className="flex min-w-0 items-center gap-1 truncate text-text-muted">
                  {isLoadingCaptureIndex && <Loader2 className="h-3 w-3 shrink-0 animate-spin" />}
                  {snippetCaptureStats.total} {snippetCaptureStats.total === 1 ? 'reference' : 'references'}
                  {snippetCaptureStats.captured > 0 ? ` · ${snippetCaptureStats.captured} captured` : ''}
                </span>
              ) : (
                <span />
              )}
              <button
                onClick={() => navigate('/references')}
                className="shrink-0 text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Open References
              </button>
            </div>

            {isLoadingSnippets ? (
              <div className="flex h-12 items-center justify-center text-text-muted">
                <Loader2 className="h-4 w-4 animate-spin" />
              </div>
            ) : filteredSnippets.length === 0 ? (
              <p className="px-4 py-3 text-xs text-text-muted">
                {roleFilteredSnippets.length === 0
                  ? 'No references yet.'
                  : 'No matches in this filter.'}
              </p>
            ) : (
              <div>
                {filteredSnippets.slice(0, 20).map((bookmark) => {
                  const isSelected = bookmark.id === selectedSnippetId;
                  const capturedReference = referenceInboxEnabled
                    ? getCapturedSnippetReference(bookmark)
                    : null;
                  return (
                    <div
                      key={bookmark.id}
                      className={`group relative transition-colors duration-fast ${
                        isSelected ? 'bg-surface-raised' : 'hover:bg-surface-raised'
                      }`}
                    >
                      {isSelected && (
                        <span
                          className="absolute inset-y-0 left-0 w-0.5 bg-accent"
                          aria-hidden="true"
                        />
                      )}
                      <button
                        onClick={() => setSelectedSnippetId(bookmark.id)}
                        className="w-full px-4 py-2.5 text-left"
                      >
                        <p className="truncate text-sm text-text-primary">
                          {bookmark.title || bookmark.conversationTitle}
                        </p>
                        <p className="mt-0.5 truncate text-xs text-text-muted">
                          {formatRoleLabel(bookmark.messageRole)}
                          {capturedReference ? ' · Captured' : ''}
                          {' · '}
                          {formatShortRelativeTime(bookmark.createdAt)}
                        </p>
                        <p className="mt-0.5 truncate text-xs text-text-tertiary">
                          {bookmark.messagePreview}
                        </p>
                      </button>
                      <div className="pointer-events-none absolute right-2 top-1.5 flex items-center gap-0.5 rounded-sm bg-surface-raised pl-2 opacity-0 transition-opacity duration-fast group-hover:pointer-events-auto group-hover:opacity-100">
                        <IconButton
                          label="Open in chat"
                          onClick={handleAsyncEvent(() => openSnippet(bookmark))}
                        >
                          <ArrowUpRight />
                        </IconButton>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}

        {isLoading && conversations.length === 0 ? (
          <div className="flex h-32 items-center justify-center text-text-muted">
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        ) : conversations.length === 0 ? (
          <p className="px-4 py-6 text-sm text-text-secondary">
            {isJournalScope ? 'No entries yet.' : 'No conversations yet.'}
          </p>
        ) : (
          <nav className="pb-2" role="navigation" aria-label="Conversations">
            {journalConversationGroups.map((group) => (
              <section key={group.key}>
                {group.label && (
                  <p className="px-4 pb-1 pt-4 text-xxs uppercase tracking-[0.08em] text-text-muted">
                    {group.label}
                  </p>
                )}
                <div>
                  {group.items.map((conversation) => {
                    const isActive = conversation.id === activeConversationId;
                    const isDeleting = deletingId === conversation.id;
                    const accent = conversation.spaceId
                      ? spaceAccentById.get(conversation.spaceId) ?? null
                      : null;
                    const conversationSpaceKind = conversation.spaceId
                      ? (spaceKindById.get(conversation.spaceId) ?? 'standard')
                      : 'standard';
                    const spaceLabel = conversation.spaceId
                      ? `${spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}${
                          conversationSpaceKind === 'journal' ? ' · Journal' : ''
                        }`
                      : '';
                    const relativeTime = formatShortRelativeTime(conversation.updatedAt);
                    const metaLine = [spaceLabel, relativeTime].filter(Boolean).join(' · ');
                    const preview =
                      conversation.lastMessagePreview
                      ?? conversation.messages?.[conversation.messages.length - 1]?.content
                      ?? '';

                    return (
                      <div
                        key={conversation.id}
                        onClick={handleAsyncEvent(async () => {
                          if (isSelectionMode) {
                            toggleConversationSelection(conversation.id);
                            return;
                          }
                          if (renamingConversationId === conversation.id) {
                            return;
                          }
                          await selectConversation(conversation.id);
                        })}
                        role="button"
                        tabIndex={0}
                        onKeyDown={handleAsyncEvent(async (e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            if (isSelectionMode) {
                              e.preventDefault();
                              toggleConversationSelection(conversation.id);
                              return;
                            }
                            if (renamingConversationId === conversation.id) {
                              return;
                            }
                            e.preventDefault();
                            await selectConversation(conversation.id);
                          }
                        })}
                        aria-label={`Select conversation: ${conversation.title}`}
                        aria-current={isActive ? 'page' : undefined}
                        className={`group relative w-full cursor-pointer px-4 py-2.5 text-left transition-colors duration-fast ${
                          isActive ? 'bg-surface-raised' : 'hover:bg-surface-raised'
                        }`}
                      >
                        {isActive && (
                          prefersReducedMotion ? (
                            <span
                              className="absolute inset-y-0 left-0 w-0.5 bg-accent"
                              aria-hidden="true"
                            />
                          ) : (
                            <motion.span
                              layoutId="sidebar-active-bar"
                              className="absolute inset-y-0 left-0 w-0.5 bg-accent"
                              aria-hidden="true"
                              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                            />
                          )
                        )}

                        <div className="flex items-center gap-2">
                          {isSelectionMode && (
                            <input
                              type="checkbox"
                              checked={selectedConversationIds.has(conversation.id)}
                              onChange={() => toggleConversationSelection(conversation.id)}
                              onClick={(e) => e.stopPropagation()}
                              className="h-3.5 w-3.5 shrink-0 cursor-pointer appearance-none rounded-sm border border-border-strong bg-transparent transition-colors duration-fast checked:border-accent checked:bg-accent checked:shadow-[inset_0_0_0_2px_hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                              aria-label={`Select conversation: ${conversation.title}`}
                            />
                          )}
                          {accent ? (
                            <span
                              className="h-2 w-2 shrink-0 rounded-full"
                              style={{ backgroundColor: accent }}
                              aria-label={conversation.spaceId ? `Space: ${spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}` : undefined}
                            />
                          ) : isJournalScope ? (
                            <NotebookPen className="h-3.5 w-3.5 shrink-0 text-text-tertiary" />
                          ) : (
                            <MessageSquare className="h-3.5 w-3.5 shrink-0 text-text-tertiary" />
                          )}

                          {renamingConversationId === conversation.id ? (
                            <div
                              className="flex min-w-0 flex-1 items-center gap-1"
                              onClick={(e) => e.stopPropagation()}
                            >
                              <input
                                autoFocus
                                value={renameDraft}
                                maxLength={120}
                                onChange={(e) => setRenameDraft(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    void saveConversationRename(
                                      conversation.id,
                                      conversation.title
                                    );
                                    return;
                                  }
                                  if (e.key === 'Escape') {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    cancelRenameConversation();
                                  }
                                }}
                                className="h-6 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-2 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
                                aria-label={`Rename conversation: ${conversation.title}`}
                              />
                              <IconButton
                                label="Save conversation title"
                                onClick={handleAsyncEvent(async (e) => {
                                  e.stopPropagation();
                                  await saveConversationRename(
                                    conversation.id,
                                    conversation.title
                                  );
                                })}
                              >
                                <Check />
                              </IconButton>
                              <IconButton
                                label="Cancel rename"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  cancelRenameConversation();
                                }}
                              >
                                <X />
                              </IconButton>
                            </div>
                          ) : (
                            <h3
                              className={`min-w-0 flex-1 truncate text-sm text-text-primary ${
                                isActive ? 'font-medium' : 'font-normal'
                              }`}
                              onDoubleClick={(e) => {
                                e.stopPropagation();
                                beginRenameConversation(conversation.id, conversation.title);
                              }}
                              title={conversation.title}
                            >
                              {conversation.title}
                            </h3>
                          )}
                        </div>

                        {metaLine && (
                          <p className="mt-0.5 truncate text-xs text-text-muted">{metaLine}</p>
                        )}
                        {preview && (
                          <p className="truncate text-xs text-text-tertiary">{preview}</p>
                        )}

                        {!isSelectionMode && renamingConversationId !== conversation.id && (
                          <div
                            className="pointer-events-none absolute right-2 top-1.5 flex items-center gap-0.5 rounded-sm bg-surface-raised pl-2 opacity-0 transition-opacity duration-fast group-hover:pointer-events-auto group-hover:opacity-100"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <IconButton
                              label={`Synthesize to Journal: ${conversation.title}`}
                              disabled={synthesizingConversationId === conversation.id}
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await synthesizeConversationToJournal(
                                  conversation.id,
                                  conversation.title,
                                );
                              })}
                            >
                              {synthesizingConversationId === conversation.id ? (
                                <Loader2 className="animate-spin" />
                              ) : (
                                <Combine />
                              )}
                            </IconButton>

                            <IconButton
                              label={`Rename conversation: ${conversation.title}`}
                              onClick={(e) => {
                                e.stopPropagation();
                                beginRenameConversation(conversation.id, conversation.title);
                              }}
                            >
                              <Pencil />
                            </IconButton>

                            <IconButton
                              label={`${conversation.isSaved ? 'Unstar' : 'Star'} conversation: ${conversation.title}`}
                              className={conversation.isSaved ? 'text-accent' : undefined}
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationSaved(conversation.id, !conversation.isSaved);
                              })}
                            >
                              <Star className={conversation.isSaved ? 'fill-current' : undefined} />
                            </IconButton>

                            <IconButton
                              label={`${conversation.isPinned ? 'Unpin' : 'Pin'} conversation: ${conversation.title}`}
                              className={conversation.isPinned ? 'text-accent' : undefined}
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationPinned(conversation.id, !conversation.isPinned);
                              })}
                            >
                              <Pin />
                            </IconButton>

                            <IconButton
                              label={`${conversation.isArchived ? 'Unarchive' : 'Archive'} conversation: ${conversation.title}`}
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationArchived(conversation.id, !conversation.isArchived);
                              })}
                            >
                              {conversation.isArchived ? <RotateCcw /> : <Archive />}
                            </IconButton>

                            <IconButton
                              label={`Delete conversation: ${conversation.title}`}
                              disabled={isDeleting}
                              className="hover:text-[hsl(var(--danger-fg))]"
                              onClick={handleAsyncEvent((e) => handleDelete(conversation.id, e))}
                            >
                              {isDeleting ? <Loader2 className="animate-spin" /> : <Trash2 />}
                            </IconButton>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </section>
            ))}
          </nav>
        )}
      </div>

      <div className="flex h-8 shrink-0 items-center justify-end border-t border-border-subtle px-4">
        <span className="text-xs text-text-muted">
          {conversations.length} {isJournalScope ? 'entry' : 'conversation'}{conversations.length !== 1 ? 's' : ''}
        </span>
      </div>

      {isSpacesOpen && createPortal(
        <>
          <motion.button
            type="button"
            aria-label="Close spaces panel"
            onClick={() => setIsSpacesOpen(false)}
            className={`fixed inset-0 bg-[hsl(var(--overlay))] ${SPACES_MODAL_LAYER_CLASSES.backdrop}`}
            initial={prefersReducedMotion ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
          />
          <motion.aside
            className={`fixed border-r border-subtle bg-surface-raised shadow-md ${SPACES_MODAL_LAYER_CLASSES.panel}`}
            style={spacesPanelFrame ?? undefined}
            initial={prefersReducedMotion ? false : { opacity: 0, scale: 0.98 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.15, ease: [0.22, 1, 0.36, 1] }}
          >
            <div className={`${SPACES_MODAL_LAYER_CLASSES.content} flex h-full flex-col overflow-hidden`}>
              <div className={`flex items-center justify-between border-b border-subtle px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <h3 className="font-serif text-sm font-semibold text-[hsl(var(--text-primary))]">Spaces</h3>
                <button
                  onClick={() => setIsSpacesOpen(false)}
                  className="rounded-sm p-1.5 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]"
                  aria-label="Close spaces panel"
                  title="Close"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className={`flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setIsCreateSpaceOpen((open) => !open)}
                    className="inline-flex h-7 items-center gap-1.5 rounded-sm px-2 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]"
                  >
                    <FolderPlus className="w-3.5 h-3.5" />
                    {isCreateSpaceOpen ? 'Cancel' : 'New space'}
                  </button>
                  <button
                    onClick={() => setIsSpaceEditorOpen((open) => !open)}
                    disabled={!selectedSpace}
                    className="inline-flex h-7 items-center gap-1.5 rounded-sm px-2 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-40"
                  >
                    <Settings2 className="w-3.5 h-3.5" />
                    Edit space
                  </button>
                </div>

                {isCreateSpaceOpen && (
                  <div className="mt-3 space-y-2 border-t border-subtle pt-3">
                    <input
                      value={newSpaceNameDraft}
                      onChange={(e) => setNewSpaceNameDraft(e.target.value)}
                      placeholder={
                        newSpaceKindDraft === 'journal'
                          ? 'Journal name (e.g. Food Research, Weekly Notes)'
                          : 'Space name (e.g. Product, Research, Personal)'
                      }
                      className="w-full rounded-sm border border-border-default bg-surface-raised px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                    />
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => setNewSpaceKindDraft('standard')}
                        className={`rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${
                          newSpaceKindDraft === 'standard'
                            ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                            : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                        }`}
                      >
                        Standard
                      </button>
                      <button
                        type="button"
                        onClick={() => setNewSpaceKindDraft('journal')}
                        className={`rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${
                          newSpaceKindDraft === 'journal'
                            ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                            : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                        }`}
                      >
                        Journal
                      </button>
                    </div>
                    <div className="flex items-center justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => {
                          setIsCreateSpaceOpen(false);
                          setNewSpaceNameDraft('');
                          setNewSpaceKindDraft('standard');
                        }}
                        className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                      >
                        Cancel
                      </button>
                      <button
                        onClick={handleAsyncEvent(createSpace)}
                        disabled={isCreatingSpace}
                        className="inline-flex items-center gap-1 rounded-sm bg-[hsl(var(--accent))] px-2 py-1 text-xs text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {isCreatingSpace ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : (
                          <Plus className="w-3.5 h-3.5" />
                        )}
                        Create
                      </button>
                    </div>
                  </div>
                )}

                <div className="mt-3 flex min-h-0 flex-1 flex-col">

                  <div className="flex-1 min-h-0 space-y-0.5 overflow-y-auto pr-1">
                    <button
                      onClick={handleAsyncEvent(() => handleSpaceSelect(null))}
                      className={`relative w-full rounded-sm px-3 py-2 text-left transition-colors duration-fast ${
                        selectedSpaceId === null ? 'bg-surface' : 'hover:bg-surface'
                      }`}
                    >
                      {selectedSpaceId === null && (
                        <span
                          className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                          aria-hidden="true"
                        />
                      )}
                      <div className="flex items-start gap-2.5">
                        <div className="flex h-6 w-6 shrink-0 items-center justify-center text-[hsl(var(--text-tertiary))]">
                          <MessageSquare className="h-4 w-4" />
                        </div>
                        <div className="min-w-0">
                          <p className="truncate text-sm text-[hsl(var(--text-primary))]">All spaces</p>
                          <p className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                            Every conversation, all scopes
                          </p>
                        </div>
                      </div>
                    </button>

                    {standardSpaces.length > 0 && (
                      <p className="px-2 pt-4 pb-1 text-xs font-medium text-[hsl(var(--text-secondary))]">
                        Spaces
                      </p>
                    )}

                    {standardSpaces.map((space) => {
                      const isSelected = selectedSpaceId === space.id;
                      const accent = normalizeHexColor(space.accentColor);

                      return (
                        <button
                          key={space.id}
                          onClick={handleAsyncEvent(() => handleSpaceSelect(space.id))}
                          className={`relative w-full rounded-sm px-3 py-2 text-left transition-colors duration-fast ${
                            isSelected ? 'bg-surface' : 'hover:bg-surface'
                          }`}
                          title={space.description ?? space.name}
                        >
                          {isSelected && (
                            <span
                              className="absolute inset-y-0 left-0 w-0.5"
                              style={{ backgroundColor: accent ?? undefined }}
                              aria-hidden="true"
                            />
                          )}
                          {isSelected && !accent && (
                            <span
                              className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                              aria-hidden="true"
                            />
                          )}
                          <div className="flex items-start gap-2.5">
                            <div className="flex h-6 w-6 shrink-0 items-center justify-center text-sm text-[hsl(var(--text-tertiary))]">
                              {space.icon || (
                                accent ? (
                                  <span
                                    className="h-2 w-2 rounded-full"
                                    style={{ backgroundColor: accent }}
                                    aria-hidden="true"
                                  />
                                ) : (
                                  <MessageSquare className="h-4 w-4" />
                                )
                              )}
                            </div>
                            <div className="min-w-0">
                              <p className="truncate text-sm text-[hsl(var(--text-primary))]">{space.name}</p>
                              {space.description ? (
                                <p className="mt-0.5 line-clamp-2 text-xs text-[hsl(var(--text-muted))]">
                                  {space.description}
                                </p>
                              ) : null}
                            </div>
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                {isSpaceEditorOpen && selectedSpace && (
                  <div className="mt-3 space-y-3 border-t border-subtle pt-3">
                    <p className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                      {selectedSpace.name}
                    </p>

                    <details open className="border-b border-subtle pb-3">
                      <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Basics</summary>
                      <div className="mt-2 space-y-2">
                        <div className="grid grid-cols-[72px_1fr] gap-2">
                          <input
                            value={spaceIconDraft}
                            onChange={(e) => setSpaceIconDraft(e.target.value)}
                            placeholder="Icon"
                            className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                          <input
                            value={spaceNameDraft}
                            onChange={(e) => setSpaceNameDraft(e.target.value)}
                            placeholder="Space name"
                            className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                        </div>
                        <div className="grid grid-cols-[92px_1fr_56px] gap-2">
                          <input
                            type="color"
                            value={normalizeHexColor(spaceAccentDraft) ?? '#8b72ff'}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            className="h-8 w-full rounded-sm border border-border-default bg-surface p-1"
                            title="Accent color"
                          />
                          <input
                            value={spaceAccentDraft}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            placeholder="#8b72ff"
                            className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                          <button
                            type="button"
                            onClick={() => setSpaceAccentDraft('')}
                            aria-label="Clear accent color"
                            title="Clear accent color"
                            className="rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                          >
                            Clear
                          </button>
                        </div>
                        <input
                          value={spaceDescriptionDraft}
                          onChange={(e) => setSpaceDescriptionDraft(e.target.value)}
                          placeholder="Description"
                          className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                        />
                      </div>
                    </details>

                    <details className="border-b border-subtle pb-3">
                      <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Defaults</summary>
                      <div className="mt-2 space-y-2">
                        <input
                          value={spaceModelDraft}
                          onChange={(e) => setSpaceModelDraft(e.target.value)}
                          placeholder="Default model id (optional)"
                          list="space-model-options"
                          className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                        />
                        <datalist id="space-model-options">
                          {availableSpaceModels.map((modelId) => (
                            <option key={modelId} value={modelId} />
                          ))}
                        </datalist>
                        <textarea
                          value={spacePromptDraft}
                          onChange={(e) => setSpacePromptDraft(e.target.value)}
                          placeholder="System prompt for this space"
                          rows={4}
                          className="w-full rounded-sm border border-border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))] resize-y"
                        />
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() => setSpaceKbDefault((value) => !value)}
                            className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${
                              spaceKbDefault
                                ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                                : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                            }`}
                          >
                            Search documents by default
                          </button>
                          <button
                            type="button"
                            onClick={() => setSpaceWebDefault((value) => !value)}
                            className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${
                              spaceWebDefault
                                ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                                : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                            }`}
                          >
                            Web by default
                          </button>
                          <button
                            type="button"
                            onClick={toggleSpaceDeepResearchDefault}
                            className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${
                              spaceDeepResearchDefault
                                ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                                : 'border-border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                            }`}
                          >
                            Deep research by default
                          </button>
                        </div>
                        {spaceDeepResearchDefault && (
                          <div
                            role="status"
                            aria-live="polite"
                            className="rounded-sm border border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] px-2 py-1.5 text-xs text-[hsl(var(--warning-fg))]"
                          >
                            Deep research is on for this space. Responses will be slower.
                          </div>
                        )}
                      </div>
                    </details>

                    <div className="flex items-center gap-1.5">
                      <button
                        onClick={handleAsyncEvent(saveSpaceEnvironment)}
                        disabled={isSavingSpace}
                        className="inline-flex items-center gap-1 rounded-sm bg-[hsl(var(--accent))] px-2.5 py-1 text-xs text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {isSavingSpace ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : (
                          <Save className="w-3.5 h-3.5" />
                        )}
                        Save
                      </button>
                      {selectedSpace.id !== 'space_general' && (
                        <button
                          onClick={handleAsyncEvent(() => setSelectedSpaceArchived(!selectedSpace.isArchived))}
                          disabled={isArchivingSpace}
                          className="inline-flex items-center gap-1 rounded-sm px-2.5 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {isArchivingSpace ? (
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Archive className="w-3.5 h-3.5" />
                          )}
                          {selectedSpace.isArchived ? 'Restore' : 'Archive'}
                        </button>
                      )}
                    </div>

                    {archivedSpaces.length > 0 && (
                      <details className="border-b border-subtle pb-3">
                        <summary className="cursor-pointer text-xs text-[hsl(var(--text-muted))]">
                          Archived spaces · {archivedSpaces.length}
                        </summary>
                        <div className="mt-2 space-y-1">
                          {archivedSpaces.slice(0, 6).map((space) => (
                            <div
                              key={space.id}
                              className="flex items-center justify-between rounded-sm border border-subtle bg-surface px-2 py-1"
                            >
                              <span className="text-xs text-[hsl(var(--text-secondary))] truncate">
                                {space.icon ? `${space.icon} ` : ''}{space.name}
                              </span>
                              <button
                                onClick={handleAsyncEvent(() => restoreArchivedSpace(space.id))}
                                disabled={isRestoringSpace}
                                className="rounded-sm border border-border-default px-1.5 py-0.5 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-fast"
                              >
                                Restore
                              </button>
                            </div>
                          ))}
                        </div>
                      </details>
                    )}
                  </div>
                )}
              </div>

              <div className={`border-t border-subtle px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <p className="text-xs text-[hsl(var(--text-muted))] text-center">
                  {activeSpacesOrdered.length} space{activeSpacesOrdered.length !== 1 ? 's' : ''}
                </p>
              </div>
            </div>
          </motion.aside>
        </>,
        document.body
      )}
    </div>
  );
}
