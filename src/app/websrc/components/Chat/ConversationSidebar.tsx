import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { differenceInDays, formatDistanceToNow, isToday, isYesterday, startOfDay } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import {
  Plus,
  MessageSquare,
  Trash2,
  Loader2,
  AlertCircle,
  X,
  Star,
  Bookmark,
  Pin,
  PanelLeft,
  Search,
  Archive,
  RotateCcw,
  ArrowUpRight,
  Save,
  Settings2,
  FolderPlus,
  NotebookPen,
  Pencil,
  Check,
} from 'lucide-react';
import { createPortal } from 'react-dom';
import { useNavigate } from 'react-router-dom';

import { useDebounce } from '../../hooks/useDebounce';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { useDownloadedModelsStore } from '../../stores/downloadedModelsStore';
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
const JOURNAL_SPACE_DEFAULT_ACCENT = '#14b8a6';

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
  'today': 'Today',
  'yesterday': 'Yesterday',
  'last-7-days': 'Last 7 days',
  'last-30-days': 'Last 30 days',
  'older': 'Older',
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

export function ConversationSidebar() {
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
  const downloadedModelMap = useDownloadedModelsStore((state) => state.downloadedModels);

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

  const selectedSnippet = useMemo(() => {
    if (!selectedSnippetId) return null;
    return filteredSnippets.find((snippet) => snippet.id === selectedSnippetId) ?? null;
  }, [filteredSnippets, selectedSnippetId]);

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

  const filterOptions: Array<{
    id: 'all' | 'saved' | 'bookmarked' | 'pinned' | 'archived' | 'snippets';
    label: string;
    icon: typeof Star;
  }> = [
    { id: 'all', label: 'All', icon: MessageSquare },
    { id: 'saved', label: 'Starred', icon: Star },
    { id: 'pinned', label: 'Pinned', icon: Pin },
    { id: 'snippets', label: 'References', icon: Save },
    { id: 'archived', label: 'Archived', icon: Archive },
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

  const selectedSnippetCapture = selectedSnippet
    ? getCapturedSnippetReference(selectedSnippet)
    : null;

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
      className={`relative h-full w-[280px] max-w-full shrink-0 bg-surface border-r border-subtle flex flex-col overflow-hidden ${
        isSpacesOpen ? SPACES_MODAL_LAYER_CLASSES.root : ''
      }`}
    >
      {/* Top rail (CHAT-REDESIGN-SPEC §5.1) */}
      <div className="flex h-12 shrink-0 items-center justify-between border-b border-subtle bg-surface px-4">
        <span className="font-serif text-base font-semibold text-[hsl(var(--text-primary))]">
          Recall
        </span>
        <button
          type="button"
          aria-label="Toggle sidebar"
          title="Toggle sidebar"
          className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
        >
          <PanelLeft className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>

      <div className="p-4 border-b border-subtle overflow-x-hidden">
        <button
          onClick={handleAsyncEvent(handleNewConversation)}
          disabled={isCreating}
          aria-label="Create new conversation"
          className="flex w-full items-center justify-center gap-2 rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isCreating ? (
            <Loader2 className="w-4 h-4 animate-spin" />
          ) : (
            <Plus className="w-4 h-4" />
          )}
          <span>{isJournalScope ? 'New entry' : 'New conversation'}</span>
        </button>

        <div className="mt-3 flex items-center justify-between gap-2 rounded-sm border border-subtle bg-surface-raised px-3 py-2">
          <div className="min-w-0">
            <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">Scope</p>
            <p className="truncate text-xs text-[hsl(var(--text-primary))]">
              {selectedSpace
                ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
                : selectedJournal
                  ? `${selectedJournal.icon ? `${selectedJournal.icon} ` : ''}${selectedJournal.name} · Journal`
                  : 'All spaces'}
            </p>
          </div>
          <div className="flex items-center gap-1.5">
            {isJournalScope && selectedJournal && (
              <button
                onClick={openSelectedJournalNotebook}
                className="inline-flex items-center gap-1.5 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
              >
                <NotebookPen className="w-3.5 h-3.5" />
                Notebook
              </button>
            )}
            <button
              onClick={() => setIsSpacesOpen(true)}
              className="inline-flex items-center gap-1.5 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
            >
              <Settings2 className="w-3.5 h-3.5" />
              Spaces
            </button>
          </div>
        </div>

        {isJournalScope && journalSpaces.length > 0 && (
          <div className="mt-2 rounded-sm border border-subtle bg-surface-raised px-3 py-2">
            <div className="flex items-start justify-between gap-2">
              <p className="text-xs text-[hsl(var(--text-secondary))]">
                Entries, pinned highlights, and notebook pages.
              </p>
              <button
                onClick={openSelectedJournalNotebook}
                className="inline-flex shrink-0 items-center gap-1 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
              >
                <NotebookPen className="h-3 w-3" />
                Open
              </button>
            </div>
          </div>
        )}

        <div className="mt-3 flex flex-wrap items-center gap-3">
          {filterOptions.map((option) => {
            const Icon = option.icon;
            const active = filterMode === option.id;
            return (
            <button
              key={option.id}
              onClick={handleAsyncEvent(() => handleFilterSelect(option.id))}
              aria-label={`${option.label} conversations`}
              aria-pressed={active}
              title={option.label}
              className={`inline-flex items-center gap-1 pb-1 text-xs transition-colors duration-fast ${
                active
                  ? 'text-[hsl(var(--text-primary))] border-b-2 border-[hsl(var(--accent))]'
                  : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))] border-b-2 border-transparent'
              }`}
            >
              <Icon className="h-3 w-3" />
              <span>{option.label}</span>
            </button>
          );
          })}
        </div>

        <div className="mt-3 relative">
          <Search className="w-4 h-4 text-[hsl(var(--text-muted))] absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            value={localQuery}
            onChange={(e) => setLocalQuery(e.target.value)}
            placeholder={isJournalScope ? 'Search entries' : 'Search conversations'}
            className="w-full h-8 pl-9 pr-3 rounded-sm border border-default bg-surface text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
          />
        </div>

        {filterMode !== 'snippets' && (
          <>
            <div className="mt-3 flex items-center justify-between gap-2">
              <button
                onClick={() => {
                  const nextSelectionMode = !isSelectionMode;
                  setIsSelectionMode(nextSelectionMode);
                  if (nextSelectionMode) {
                    void loadJournals(false);
                  }
                }}
                className={`inline-flex items-center gap-1.5 rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${
                  isSelectionMode
                    ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                    : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                }`}
              >
                {isSelectionMode ? (
                  <>
                    <X className="h-3 w-3" />
                    Done
                  </>
                ) : (
                  <>
                    <Bookmark className="h-3 w-3" />
                    Select multiple
                  </>
                )}
              </button>

              {isSelectionMode && (
                <button
                  onClick={toggleSelectAllVisibleConversations}
                  className="inline-flex items-center gap-1 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                >
                  {areAllVisibleConversationsSelected ? 'Clear' : 'Select all'}
                </button>
              )}
            </div>

            {isSelectionMode && (
              <div className="mt-2 rounded-sm border border-subtle bg-surface-raised px-3 py-2.5">
                <p className="text-xs text-[hsl(var(--text-secondary))]">
                  {selectedConversationCount} selected
                </p>
                <div className="mt-2 flex items-center gap-1.5">
                  <select
                    value={bulkSpaceIdDraft}
                    onChange={(e) => setBulkSpaceIdDraft(e.target.value)}
                    disabled={isLoadingJournals}
                    className="min-w-0 flex-1 rounded-sm border border-default bg-surface px-2 py-1 text-xs text-[hsl(var(--text-primary))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                  >
                    {isLoadingJournals ? (
                      <option value="" disabled>
                        Loading...
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
                    onClick={handleAsyncEvent(addSelectedConversationsToJournal)}
                    disabled={
                      selectedConversationCount === 0
                      || !bulkSpaceIdDraft
                      || isBulkMoving
                      || Boolean(journalsLoadError)
                      || journalSpaces.length === 0
                    }
                    className="inline-flex items-center gap-1 rounded-sm border border-default bg-surface px-2 py-1 text-xs text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {isBulkMoving ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : (
                      <ArrowUpRight className="h-3 w-3" />
                    )}
                    Add
                  </button>
                </div>
                {journalsLoadError ? (
                  <p className="mt-2 text-xs text-[hsl(var(--danger-fg))]">
                    Couldn't load journals. {journalsLoadError}
                  </p>
                ) : journalSpaces.length === 0 && (
                  <div className="mt-2 space-y-1.5">
                    <p className="text-xs text-[hsl(var(--text-muted))]">
                      No journals yet. Create one to organize selected conversations.
                    </p>
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={handleAsyncEvent(createQuickJournal)}
                        disabled={isCreatingQuickJournal}
                        className="inline-flex items-center gap-1 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        {isCreatingQuickJournal ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                          <Plus className="h-3 w-3" />
                        )}
                        Create journal
                      </button>
                      <button
                        type="button"
                        onClick={() => navigate('/journals')}
                        className="inline-flex items-center gap-1 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                      >
                        <ArrowUpRight className="h-3 w-3" />
                        Open journals
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>

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
          <div className="border-b border-subtle p-2.5">
            <div className="rounded-sm border border-subtle bg-surface-raised p-3">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="text-sm font-medium text-[hsl(var(--text-primary))]">
                    References
                  </p>
                  <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">
                    Review and process them below.
                  </p>
                </div>
                <span className="inline-flex items-center gap-1 rounded-sm border border-subtle bg-surface px-2 py-0.5 text-xs text-[hsl(var(--text-secondary))]">
                  <Bookmark className="h-3 w-3" />
                  {snippetResults.length}
                </span>
              </div>

              <div className="mt-3 flex items-center justify-between gap-2">
                <div className="flex items-center gap-1.5">
                  {([
                    ['all', 'All'],
                    ['assistant', 'Assistant'],
                    ['user', 'You'],
                    ['system', 'System'],
                  ] as const).map(([value, label]) => (
                    <button
                      key={value}
                      onClick={() => setSnippetRoleFilter(value)}
                      className={`rounded-sm border px-2 py-0.5 text-xs transition-colors duration-fast ${
                        snippetRoleFilter === value
                          ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                          : 'border-default text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))]'
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                <button
                  onClick={() => navigate('/references')}
                  className="inline-flex items-center gap-1 rounded-sm border border-default px-2 py-0.5 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                >
                  <ArrowUpRight className="h-3 w-3" />
                  Inbox
                </button>
              </div>

              {referenceInboxEnabled && (
                <div className="mt-2 inline-flex items-center gap-1 text-xs text-[hsl(var(--text-muted))]">
                  {isLoadingCaptureIndex && <Loader2 className="h-3 w-3 animate-spin" />}
                  {snippetCaptureStats.pending} pending · {snippetCaptureStats.captured} captured
                </div>
              )}
            </div>

            {isLoadingSnippets ? (
              <div className="h-12 flex items-center justify-center text-[hsl(var(--text-muted))]">
                <Loader2 className="h-4 w-4 animate-spin" />
              </div>
            ) : filteredSnippets.length === 0 ? (
              <div className="px-2 py-3 text-xs text-[hsl(var(--text-muted))]">
                {roleFilteredSnippets.length === 0
                  ? 'No references yet. Reference any message with the bookmark icon.'
                  : 'No matches in this filter.'}
              </div>
            ) : (
              <div className="mt-2 space-y-2">
                <div className="space-y-1">
                  {filteredSnippets.slice(0, 20).map((bookmark) => {
                    const isSelected = bookmark.id === selectedSnippetId;
                    const capturedReference = referenceInboxEnabled
                      ? getCapturedSnippetReference(bookmark)
                      : null;
                    return (
                      <div
                        key={bookmark.id}
                        className={`relative rounded-sm transition-colors duration-fast ${
                          isSelected
                            ? 'bg-surface-raised'
                            : 'hover:bg-surface-raised'
                        }`}
                      >
                        {isSelected && (
                          <span
                            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                            aria-hidden="true"
                          />
                        )}
                        <button
                          onClick={() => setSelectedSnippetId(bookmark.id)}
                          className="w-full text-left px-3 pt-2.5 pb-2"
                        >
                          <div className="mb-1 flex items-center justify-between gap-2">
                            <div className="flex items-center gap-1.5">
                              <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-tertiary))] truncate">
                                {formatRoleLabel(bookmark.messageRole)}
                              </p>
                              <span
                                className={`rounded-sm border px-1.5 py-0 text-xxs uppercase tracking-[0.04em] ${
                                  capturedReference
                                    ? 'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]'
                                    : 'border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))]'
                                }`}
                              >
                                {capturedReference ? 'Captured' : 'Pending'}
                              </span>
                            </div>
                            <p className="text-xs text-[hsl(var(--text-muted))]">
                              {new Date(bookmark.createdAt).toLocaleDateString()}
                            </p>
                          </div>
                          <p className="text-sm text-[hsl(var(--text-primary))] truncate">
                            {bookmark.title || bookmark.conversationTitle}
                          </p>
                          <p className="mt-0.5 text-xs text-[hsl(var(--text-tertiary))] truncate">
                            <span className="inline-flex items-center gap-1">
                              {(() => {
                                const accent = spaceAccentById.get(bookmark.spaceId) ?? null;
                                return accent ? (
                                  <span
                                    className="inline-block h-2 w-2 rounded-full"
                                    style={{ backgroundColor: accent }}
                                    aria-hidden="true"
                                  />
                                ) : null;
                              })()}
                              {bookmark.conversationTitle}
                            </span>
                          </p>
                          <p className="mt-1.5 text-xs text-[hsl(var(--text-muted))] line-clamp-2 break-words">
                            {bookmark.messagePreview}
                          </p>
                        </button>
                        <div className="px-3 pb-2.5">
                          <button
                            onClick={handleAsyncEvent(() => openSnippet(bookmark))}
                            className="text-xs inline-flex items-center gap-1 rounded-sm border border-default px-2 py-0.5 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                          >
                            <ArrowUpRight className="w-3 h-3" />
                            Open in chat
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>

                {selectedSnippet && (
                  <div className="rounded-sm border border-subtle bg-surface-raised p-3">
                    <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                      Selected reference
                    </p>
                    <p className="mt-1 text-sm text-[hsl(var(--text-primary))] line-clamp-2">
                      {selectedSnippet.title || selectedSnippet.conversationTitle}
                    </p>
                    <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">
                      {selectedSnippetCapture
                        ? `Captured in ${selectedSnippetCapture.noteTitle}.`
                        : 'Pending capture.'}
                    </p>
                    <button
                      onClick={() => navigate('/references')}
                      className="mt-2 w-full inline-flex items-center justify-center gap-1 text-xs rounded-sm border border-default px-2 py-1 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                    >
                      <ArrowUpRight className="w-3 h-3" />
                      Manage references
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {isLoading && conversations.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-[hsl(var(--text-muted))]">
            <Loader2 className="w-5 h-5 animate-spin" />
          </div>
        ) : conversations.length === 0 ? (
          <div className="flex flex-col items-center justify-center min-h-[8rem] px-6 py-8 text-center">
            <MessageSquare className="w-6 h-6 mb-3 text-[hsl(var(--text-muted))]" />
            {isJournalScope ? (
              <>
                <p className="text-sm text-[hsl(var(--text-secondary))]">No entries in this journal.</p>
                <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">Each entry is a day's conversation.</p>
              </>
            ) : (
              <>
                <p className="text-sm text-[hsl(var(--text-secondary))]">No conversations here.</p>
                <p className="mt-1 text-xs text-[hsl(var(--text-muted))]">Every question you ask lives in a conversation.</p>
              </>
            )}
          </div>
        ) : (
          <nav
            className="p-2 pr-3"
            role="navigation"
            aria-label="Conversations"
          >
            {journalConversationGroups.map((group, groupIdx) => (
              <section
                key={group.key}
                className={`space-y-0.5 ${!isJournalScope && groupIdx > 0 ? 'mt-4' : ''}`}
              >
                {isJournalScope && group.label && (
                  <p className="px-2 py-1 text-xs italic font-serif text-[hsl(var(--text-tertiary))]">
                    {group.label}
                  </p>
                )}
                {!isJournalScope && group.label && (
                  <p className="px-4 py-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                    {group.label}
                  </p>
                )}
                <div className="space-y-0.5">
                  {group.items.map((conversation, groupIndex) => {
                    const isActive = conversation.id === activeConversationId;
                    const isDeleting = deletingId === conversation.id;
                    const accent = conversation.spaceId
                      ? spaceAccentById.get(conversation.spaceId) ?? null
                      : null;
                    const conversationSpaceKind = conversation.spaceId
                      ? (spaceKindById.get(conversation.spaceId) ?? 'standard')
                      : 'standard';

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
                        className={`group relative w-full text-left px-4 py-2 rounded-sm transition-colors duration-fast cursor-pointer ${
                          isActive
                            ? 'bg-surface-raised'
                            : 'hover:bg-surface-raised'
                        }`}
                      >
                        {isActive && (
                          prefersReducedMotion ? (
                            <span
                              className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                              aria-hidden="true"
                            />
                          ) : (
                            <motion.span
                              layoutId="sidebar-active-bar"
                              className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
                              aria-hidden="true"
                              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                            />
                          )
                        )}
                        <div className="relative flex items-start gap-2">
                          <div className={`flex-1 min-w-0 transition-[padding] duration-fast ${isSelectionMode ? '' : 'pr-28'}`}>
                            {isJournalScope && (
                              <p className="mb-1 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                                Entry {groupIndex + 1}
                              </p>
                            )}
                            <div className="flex items-center gap-2 mb-0.5">
                              {isSelectionMode && (
                                <input
                                  type="checkbox"
                                  checked={selectedConversationIds.has(conversation.id)}
                                  onChange={() => toggleConversationSelection(conversation.id)}
                                  onClick={(e) => e.stopPropagation()}
                                  className="h-3.5 w-3.5 rounded-sm border-default bg-transparent accent-[hsl(var(--accent))]"
                                  aria-label={`Select conversation: ${conversation.title}`}
                                />
                              )}
                              {/* Per-space 8x8 identity dot (CHAT-REDESIGN-SPEC §5.4) */}
                              {accent ? (
                                <span
                                  className="h-2 w-2 shrink-0 rounded-full"
                                  style={{ backgroundColor: accent }}
                                  aria-label={conversation.spaceId ? `Space: ${spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}` : undefined}
                                />
                              ) : isJournalScope ? (
                                <NotebookPen className="w-3.5 h-3.5 text-[hsl(var(--text-tertiary))] flex-shrink-0" />
                              ) : (
                                <MessageSquare className="w-3.5 h-3.5 text-[hsl(var(--text-tertiary))] flex-shrink-0" />
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
                                    className="h-6 min-w-0 flex-1 rounded-sm border border-default bg-surface px-2 text-xs text-[hsl(var(--text-primary))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                                    aria-label={`Rename conversation: ${conversation.title}`}
                                  />
                                  <button
                                    onClick={handleAsyncEvent(async (e) => {
                                      e.stopPropagation();
                                      await saveConversationRename(
                                        conversation.id,
                                        conversation.title
                                      );
                                    })}
                                    className="rounded-sm border border-default p-1 text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--accent))]"
                                    aria-label="Save conversation title"
                                    title="Save title"
                                  >
                                    <Check className="h-3 w-3" />
                                  </button>
                                  <button
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      cancelRenameConversation();
                                    }}
                                    className="rounded-sm border border-default p-1 text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                                    aria-label="Cancel rename"
                                    title="Cancel"
                                  >
                                    <X className="h-3 w-3" />
                                  </button>
                                </div>
                              ) : (
                                <h3
                                  className={`text-sm line-clamp-1 break-words ${
                                    isActive
                                      ? 'font-medium text-[hsl(var(--text-primary))]'
                                      : 'font-normal text-[hsl(var(--text-primary))]'
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
                            {conversation.spaceId && (
                              <p className="text-xs text-[hsl(var(--text-muted))] truncate">
                                {spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}
                                {conversationSpaceKind === 'journal' && ' · Journal'}
                              </p>
                            )}
                            <p className="text-xs text-[hsl(var(--text-muted))] line-clamp-1">
                              {(() => {
                                const date = new Date(conversation.updatedAt);
                                return isNaN(date.getTime()) ? 'Recently' : formatDistanceToNow(date, { addSuffix: true });
                              })()}
                            </p>
                          </div>

                          <div className={`${
                            isSelectionMode
                              ? 'hidden'
                              : 'absolute right-0 top-0 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto'
                          } transition-opacity duration-fast flex items-center gap-0.5`}>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                beginRenameConversation(conversation.id, conversation.title);
                              }}
                              aria-label={`Rename conversation: ${conversation.title}`}
                              className="p-1 rounded-sm text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                              title="Rename conversation"
                            >
                              <Pencil className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationSaved(conversation.id, !conversation.isSaved);
                              })}
                              aria-label={`${conversation.isSaved ? 'Unstar' : 'Star'} conversation: ${conversation.title}`}
                              className={`p-1 rounded-sm transition-colors duration-fast ${
                                conversation.isSaved
                                  ? 'text-[hsl(var(--accent))]'
                                  : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]'
                              }`}
                              title={conversation.isSaved ? 'Unstar conversation' : 'Star conversation'}
                            >
                              <Star className={`w-3.5 h-3.5 ${conversation.isSaved ? 'fill-current' : ''}`} />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationPinned(conversation.id, !conversation.isPinned);
                              })}
                              aria-label={`${conversation.isPinned ? 'Unpin' : 'Pin'} conversation: ${conversation.title}`}
                              className={`p-1 rounded-sm transition-colors duration-fast ${
                                conversation.isPinned
                                  ? 'text-[hsl(var(--accent))]'
                                  : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]'
                              }`}
                              title={conversation.isPinned ? 'Unpin conversation' : 'Pin conversation'}
                            >
                              <Pin className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationArchived(conversation.id, !conversation.isArchived);
                              })}
                              aria-label={`${conversation.isArchived ? 'Unarchive' : 'Archive'} conversation: ${conversation.title}`}
                              className="p-1 rounded-sm text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                              title={conversation.isArchived ? 'Unarchive' : 'Archive'}
                            >
                              {conversation.isArchived ? (
                                <RotateCcw className="w-3.5 h-3.5" />
                              ) : (
                                <Archive className="w-3.5 h-3.5" />
                              )}
                            </button>

                            <button
                              onClick={handleAsyncEvent((e) => handleDelete(conversation.id, e))}
                              disabled={isDeleting}
                              aria-label={`Delete conversation: ${conversation.title}`}
                              className="p-1 rounded-sm text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--danger-fg))] disabled:opacity-50 transition-colors duration-fast"
                              title="Delete conversation"
                            >
                              {isDeleting ? (
                                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                              ) : (
                                <Trash2 className="w-3.5 h-3.5" />
                              )}
                            </button>
                          </div>
                        </div>

                        {(conversation.lastMessagePreview || (conversation.messages && conversation.messages.length > 0)) && (
                          <p className="mt-1 text-xs text-[hsl(var(--text-muted))] line-clamp-2 break-words">
                            {conversation.lastMessagePreview ?? conversation.messages?.[conversation.messages.length - 1]?.content}
                          </p>
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

      <div className="p-3 border-t border-subtle">
        <div className="text-xs text-[hsl(var(--text-muted))] text-center">
          {conversations.length} {isJournalScope ? 'entry' : 'conversation'}{conversations.length !== 1 ? 's' : ''}
        </div>
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
                <div>
                  <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">Per-space context</p>
                  <h3 className="text-sm font-medium text-[hsl(var(--text-primary))]">Spaces</h3>
                </div>
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
                    className="inline-flex items-center gap-1.5 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                  >
                    <FolderPlus className="w-3.5 h-3.5" />
                    {isCreateSpaceOpen ? 'Cancel' : 'New space'}
                  </button>
                  <button
                    onClick={() => setIsSpaceEditorOpen((open) => !open)}
                    disabled={!selectedSpace}
                    className="inline-flex items-center gap-1.5 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-40"
                  >
                    <Settings2 className="w-3.5 h-3.5" />
                    Edit space
                  </button>
                </div>

                {isCreateSpaceOpen && (
                  <div className="mt-2 space-y-2 rounded-sm border border-subtle bg-surface p-2.5">
                    <input
                      value={newSpaceNameDraft}
                      onChange={(e) => setNewSpaceNameDraft(e.target.value)}
                      placeholder={
                        newSpaceKindDraft === 'journal'
                          ? 'Journal name (e.g. Food Research, Weekly Notes)'
                          : 'Space name (e.g. Product, Research, Personal)'
                      }
                      className="w-full rounded-sm border border-default bg-surface-raised px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                    />
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => setNewSpaceKindDraft('standard')}
                        className={`rounded-sm border px-2 py-1 text-xs transition-colors duration-fast ${
                          newSpaceKindDraft === 'standard'
                            ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                            : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
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
                            : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
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
                        className="inline-flex items-center gap-1 rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
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
                  <p className="mb-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                    Choose space
                  </p>
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
                      <p className="px-2 pt-3 pb-1 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
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
                              ) : (
                                <p className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                                  No description
                                </p>
                              )}
                            </div>
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                {isSpaceEditorOpen && selectedSpace && (
                  <div className="mt-2 space-y-2 rounded-sm border border-subtle bg-surface p-3">
                    <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                      Settings
                    </p>

                    <details open className="rounded-sm border border-subtle bg-surface-raised p-2">
                      <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Basics</summary>
                      <div className="mt-2 space-y-2">
                        <div className="grid grid-cols-[72px_1fr] gap-2">
                          <input
                            value={spaceIconDraft}
                            onChange={(e) => setSpaceIconDraft(e.target.value)}
                            placeholder="Icon"
                            className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                          <input
                            value={spaceNameDraft}
                            onChange={(e) => setSpaceNameDraft(e.target.value)}
                            placeholder="Space name"
                            className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                        </div>
                        <div className="grid grid-cols-[92px_1fr_56px] gap-2">
                          <input
                            type="color"
                            value={normalizeHexColor(spaceAccentDraft) ?? '#8b72ff'}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            className="h-8 w-full rounded-sm border border-default bg-surface p-1"
                            title="Accent color"
                          />
                          <input
                            value={spaceAccentDraft}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            placeholder="#8b72ff"
                            className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                          />
                          <button
                            type="button"
                            onClick={() => setSpaceAccentDraft('')}
                            aria-label="Clear accent color"
                            title="Clear accent color"
                            className="rounded-sm border border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                          >
                            Clear
                          </button>
                        </div>
                        <input
                          value={spaceDescriptionDraft}
                          onChange={(e) => setSpaceDescriptionDraft(e.target.value)}
                          placeholder="Description"
                          className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                        />
                      </div>
                    </details>

                    <details className="rounded-sm border border-subtle bg-surface-raised p-2">
                      <summary className="cursor-pointer text-xs text-[hsl(var(--text-secondary))]">Defaults</summary>
                      <div className="mt-2 space-y-2">
                        <input
                          value={spaceModelDraft}
                          onChange={(e) => setSpaceModelDraft(e.target.value)}
                          placeholder="Default model id (optional)"
                          list="space-model-options"
                          className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
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
                          className="w-full rounded-sm border border-default bg-surface px-2 py-1.5 text-xs text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))] resize-y"
                        />
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() => setSpaceKbDefault((value) => !value)}
                            className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${
                              spaceKbDefault
                                ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                                : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
                            }`}
                          >
                            Knowledge base by default
                          </button>
                          <button
                            type="button"
                            onClick={() => setSpaceWebDefault((value) => !value)}
                            className={`px-2 py-1 text-xs rounded-sm border transition-colors duration-fast ${
                              spaceWebDefault
                                ? 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]'
                                : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
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
                                : 'border-default text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]'
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
                          className="inline-flex items-center gap-1 rounded-sm border border-default px-2.5 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed"
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
                      <details className="rounded-sm border border-subtle bg-surface-raised p-2">
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
                                className="rounded-sm border border-default px-1.5 py-0.5 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-fast"
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
