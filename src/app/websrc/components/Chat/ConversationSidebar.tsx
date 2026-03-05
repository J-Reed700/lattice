import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
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

const scrollToMessage = (messageId: string) => {
  let attempts = 0;
  const maxAttempts = 12;

  const tick = () => {
    const element = document.getElementById(`message-${messageId}`);
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' });
      element.classList.add('ring-2', 'ring-blue-400/70');
      window.setTimeout(() => {
        element.classList.remove('ring-2', 'ring-blue-400/70');
      }, 1200);
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

const hexToRgb = (hexColor: string): { r: number; g: number; b: number } => {
  const hex = hexColor.replace('#', '');
  const normalized = hex.length === 3
    ? `${hex[0]}${hex[0]}${hex[1]}${hex[1]}${hex[2]}${hex[2]}`
    : hex;
  const num = Number.parseInt(normalized, 16);
  return {
    r: (num >> 16) & 255,
    g: (num >> 8) & 255,
    b: num & 255,
  };
};

const withAlpha = (hexColor: string, alpha: number): string => {
  const { r, g, b } = hexToRgb(hexColor);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
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
    setConversationBookmarked,
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
        toast.error('Failed to load journals', {
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
      return [{
        key: 'all-conversations',
        label: null as string | null,
        items: conversations,
      }];
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
        toast.error('Failed to create journal', {
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
    { id: 'saved', label: 'Saved', icon: Star },
    { id: 'bookmarked', label: 'Bookmarked', icon: Bookmark },
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
      className={`relative h-full w-[clamp(16rem,28vw,20rem)] max-w-full shrink-0 bg-black/30 backdrop-blur-2xl border-r border-white/[0.06] flex flex-col overflow-hidden ${
        isSpacesOpen ? SPACES_MODAL_LAYER_CLASSES.root : ''
      }`}
    >
      <div className="p-4 border-b border-white/10 overflow-x-hidden">
        <button
          onClick={handleAsyncEvent(handleNewConversation)}
          disabled={isCreating}
          aria-label="Create new conversation"
          className="w-full px-4 py-3 bg-white/[0.04] hover:bg-gradient-to-r hover:from-blue-500/15 hover:to-indigo-500/15 border border-white/[0.08] hover:border-blue-500/30 rounded-xl text-white/80 hover:text-white/90 font-medium transition-all duration-300 flex items-center justify-center gap-2 backdrop-blur-sm disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {isCreating ? (
            <Loader2 className="w-4 h-4 animate-spin" />
          ) : (
            <Plus className="w-4 h-4" />
          )}
          <span>{isJournalScope ? 'New Journal Entry' : 'New Conversation'}</span>
        </button>

        <div className="mt-3 flex items-center justify-between gap-2 rounded-lg border border-white/10 bg-white/[0.02] px-3 py-2">
          <div className="min-w-0">
            <p className="text-[10px] uppercase tracking-wide text-white/45">Space Scope</p>
            <p className="truncate text-xs text-white/80">
              {selectedSpace
                ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
                : selectedJournal
                  ? `${selectedJournal.icon ? `${selectedJournal.icon} ` : ''}${selectedJournal.name} · Journal`
                  : 'All Spaces'}
            </p>
          </div>
          <div className="flex items-center gap-1.5">
            {isJournalScope && selectedJournal && (
              <button
                onClick={openSelectedJournalNotebook}
                className="inline-flex items-center gap-1.5 rounded-md border border-emerald-400/35 bg-emerald-500/12 px-2.5 py-1.5 text-[11px] text-emerald-100 transition-colors hover:border-emerald-300/60"
              >
                <NotebookPen className="w-3.5 h-3.5" />
                Open Notebook
              </button>
            )}
            <button
              onClick={() => setIsSpacesOpen(true)}
              className="inline-flex items-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-1.5 text-[11px] text-white/70 transition-colors hover:border-white/30 hover:text-white/90"
            >
              <Settings2 className="w-3.5 h-3.5" />
              Spaces
            </button>
          </div>
        </div>

        {isJournalScope && (
          <div className="mt-2 rounded-lg border border-emerald-300/30 bg-emerald-500/10 px-3 py-2">
            <div className="flex items-start justify-between gap-2">
              <div>
                <p className="text-[10px] uppercase tracking-wide text-emerald-100/70">Notebook Mode</p>
                <p className="mt-1 text-xs text-emerald-100/90">
                  Journal v2 is active: entries, pinned highlights, and notebook pages.
                </p>
              </div>
              {journalSpaces.length > 0 && (
                <button
                  onClick={openSelectedJournalNotebook}
                  className="inline-flex shrink-0 items-center gap-1 rounded-md border border-emerald-300/40 bg-emerald-500/15 px-2 py-1 text-[11px] text-emerald-100 hover:border-emerald-200/70"
                >
                  <NotebookPen className="h-3 w-3" />
                  Open
                </button>
              )}
            </div>
          </div>
        )}

        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          {filterOptions.map((option) => {
            const Icon = option.icon;
            return (
            <button
              key={option.id}
              onClick={handleAsyncEvent(() => handleFilterSelect(option.id))}
              aria-label={`${option.label} conversations`}
              aria-pressed={filterMode === option.id}
              title={option.label}
              className={`inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border transition-colors ${
                filterMode === option.id
                  ? 'bg-white/15 border-white/30 text-white'
                  : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
              }`}
            >
              <Icon className="h-3.5 w-3.5" />
            </button>
          );
          })}
        </div>

        <div className="mt-3 relative">
          <Search className="w-4 h-4 text-white/40 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            value={localQuery}
            onChange={(e) => setLocalQuery(e.target.value)}
            placeholder={isJournalScope ? 'Search journal entries...' : 'Search conversations...'}
            className="w-full pl-9 pr-3 py-2.5 rounded-lg border border-white/10 bg-white/[0.03] text-sm text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
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
                className={`inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-[11px] transition-colors ${
                  isSelectionMode
                    ? 'border-blue-400/45 bg-blue-500/20 text-blue-100'
                    : 'border-white/15 bg-white/5 text-white/70 hover:border-white/30 hover:text-white/90'
                }`}
              >
                {isSelectionMode ? (
                  <>
                    <X className="h-3.5 w-3.5" />
                    Done Selecting
                  </>
                ) : (
                  <>
                    <Bookmark className="h-3.5 w-3.5" />
                    Select Multiple
                  </>
                )}
              </button>

              {isSelectionMode && (
                <button
                  onClick={toggleSelectAllVisibleConversations}
                  className="inline-flex items-center gap-1 rounded-md border border-white/15 bg-white/5 px-2 py-1 text-[11px] text-white/70 transition-colors hover:border-white/30 hover:text-white"
                >
                  {areAllVisibleConversationsSelected ? 'Clear Page' : 'Select Page'}
                </button>
              )}
            </div>

            {isSelectionMode && (
              <div className="mt-2 rounded-lg border border-blue-400/20 bg-blue-500/8 px-2.5 py-2.5">
                <p className="text-[11px] text-blue-100/85">
                  {selectedConversationCount} selected
                </p>
                <div className="mt-2 flex items-center gap-1.5">
                  <select
                    value={bulkSpaceIdDraft}
                    onChange={(e) => setBulkSpaceIdDraft(e.target.value)}
                    disabled={isLoadingJournals}
                    className="min-w-0 flex-1 rounded-md border border-white/15 bg-white/[0.05] px-2 py-1.5 text-[11px] text-white/85 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                  >
                    {isLoadingJournals ? (
                      <option value="" disabled>
                        Loading journals...
                      </option>
                    ) : journalsLoadError ? (
                      <option value="" disabled>
                        Failed to load journals
                      </option>
                    ) : journalSpaces.length === 0 ? (
                      <option value="" disabled>
                        No journals yet
                      </option>
                    ) : (
                      <>
                        <option value="" disabled>
                          Choose journal...
                        </option>
                        {journalSpaces.map((space) => (
                          <option key={space.id} value={space.id} className="bg-slate-900 text-white">
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
                    className="inline-flex items-center gap-1 rounded-md border border-emerald-400/40 bg-emerald-500/15 px-2.5 py-1.5 text-[11px] text-emerald-100 transition-colors hover:border-emerald-300/60 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {isBulkMoving ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <ArrowUpRight className="h-3.5 w-3.5" />
                    )}
                    Add to Journal
                  </button>
                </div>
                {journalsLoadError ? (
                  <p className="mt-2 text-[11px] text-rose-200/85">
                    Failed to load journals: {journalsLoadError}
                  </p>
                ) : journalSpaces.length === 0 && (
                  <div className="mt-2 space-y-1.5">
                    <p className="text-[11px] text-blue-100/65">
                      No journals yet. Create one to organize selected conversations.
                    </p>
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={handleAsyncEvent(createQuickJournal)}
                        disabled={isCreatingQuickJournal}
                        className="inline-flex items-center gap-1 rounded-md border border-blue-400/40 bg-blue-500/15 px-2 py-1 text-[11px] text-blue-100 transition-colors hover:border-blue-300/65 disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        {isCreatingQuickJournal ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <Plus className="h-3.5 w-3.5" />
                        )}
                        Create Journal
                      </button>
                      <button
                        type="button"
                        onClick={() => navigate('/journals')}
                        className="inline-flex items-center gap-1 rounded-md border border-emerald-400/40 bg-emerald-500/15 px-2 py-1 text-[11px] text-emerald-100 transition-colors hover:border-emerald-300/60"
                      >
                        <ArrowUpRight className="h-3.5 w-3.5" />
                        Open Journals
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
        <div className="mx-4 mt-4 bg-red-500/10 border border-red-500/20 rounded-lg px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <AlertCircle className="w-5 h-5 text-red-400" />
            <p className="text-sm text-red-300">{error}</p>
          </div>
          <button
            onClick={clearError}
            className="text-red-400/60 hover:text-red-400 transition-colors"
            aria-label="Clear error"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        {filterMode === 'snippets' && (
          <div className="border-b border-white/10 bg-[linear-gradient(135deg,rgba(34,211,238,0.08),rgba(251,191,36,0.06)_35%,transparent_72%)] p-2.5">
            <div className="rounded-xl border border-white/10 bg-black/25 p-3 backdrop-blur-sm">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="text-[11px] uppercase tracking-[0.18em] text-cyan-100/65">
                    Knowledge Capture
                  </p>
                  <p className="mt-1 text-sm font-medium text-white/90">
                    Saved References
                  </p>
                  <p className="mt-1 text-[11px] text-white/55">
                    Review and process references in Reference Inbox.
                  </p>
                </div>
                <span className="inline-flex items-center gap-1 rounded-full border border-cyan-300/35 bg-cyan-400/15 px-2.5 py-1 text-[11px] text-cyan-100">
                  <Bookmark className="h-3 w-3" />
                  {snippetResults.length}
                </span>
              </div>

              <div className="mt-3 flex items-center justify-between gap-2">
                <div className="flex items-center gap-1.5">
                  {([
                    ['all', 'All'],
                    ['assistant', 'AI'],
                    ['user', 'You'],
                    ['system', 'System'],
                  ] as const).map(([value, label]) => (
                    <button
                      key={value}
                      onClick={() => setSnippetRoleFilter(value)}
                      className={`rounded-full border px-2.5 py-1 text-[11px] transition-colors ${
                        snippetRoleFilter === value
                          ? 'border-cyan-300/60 bg-cyan-400/18 text-cyan-100'
                          : 'border-white/15 bg-white/5 text-white/60 hover:text-white/85 hover:border-white/30'
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                <button
                  onClick={() => navigate('/references')}
                  className="inline-flex items-center gap-1 rounded-full border border-emerald-300/30 bg-emerald-500/10 px-2.5 py-1 text-[11px] text-emerald-100/85 transition-colors hover:border-emerald-300/55 hover:text-emerald-100"
                >
                  <ArrowUpRight className="h-3 w-3" />
                  Inbox
                </button>
              </div>

              {referenceInboxEnabled && (
                <div className="mt-2 inline-flex items-center gap-1 text-[11px] text-white/55">
                  {isLoadingCaptureIndex && <Loader2 className="h-3 w-3 animate-spin" />}
                  {snippetCaptureStats.pending} pending · {snippetCaptureStats.captured} captured
                </div>
              )}
            </div>

            {isLoadingSnippets ? (
              <div className="h-12 flex items-center justify-center text-white/45">
                <Loader2 className="h-4 w-4 animate-spin" />
              </div>
            ) : filteredSnippets.length === 0 ? (
              <div className="px-2 py-3 text-xs text-white/45">
                {roleFilteredSnippets.length === 0
                  ? 'No saved references yet.'
                  : 'No references match the selected role filter.'}
              </div>
            ) : (
              <div className="space-y-2">
                <div className="space-y-1">
                  {filteredSnippets.slice(0, 20).map((bookmark) => {
                    const isSelected = bookmark.id === selectedSnippetId;
                    const capturedReference = referenceInboxEnabled
                      ? getCapturedSnippetReference(bookmark)
                      : null;
                    return (
                      <div
                        key={bookmark.id}
                        className={`rounded-xl border transition-colors ${
                          isSelected
                            ? 'border-cyan-300/45 bg-cyan-500/10 shadow-[0_0_0_1px_rgba(34,211,238,0.18)]'
                            : 'border-white/10 bg-white/[0.02] hover:border-white/25'
                        }`}
                      >
                        <button
                          onClick={() => setSelectedSnippetId(bookmark.id)}
                          className="w-full text-left px-3 pt-2.5 pb-2"
                        >
                          <div className="mb-1 flex items-center justify-between gap-2">
                            <div className="flex items-center gap-1.5">
                              <p className="text-xs text-cyan-200 truncate">
                                {bookmark.messageRole.toUpperCase()}
                              </p>
                              <span
                                className={`rounded-full border px-1.5 py-0.5 text-[10px] uppercase tracking-wide ${
                                  capturedReference
                                    ? 'border-emerald-300/45 bg-emerald-500/15 text-emerald-100'
                                    : 'border-amber-300/40 bg-amber-500/15 text-amber-100'
                                }`}
                              >
                                {capturedReference ? 'Captured' : 'Pending'}
                              </span>
                            </div>
                            <p className="text-[11px] text-white/45">
                              {new Date(bookmark.createdAt).toLocaleDateString()}
                            </p>
                          </div>
                          <p className="text-xs text-white/90 truncate">
                            {bookmark.title || bookmark.conversationTitle}
                          </p>
                          <p className="mt-0.5 text-[11px] text-white/40 truncate">
                            <span
                              className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-white/15 bg-white/5"
                              style={(() => {
                                const accent = spaceAccentById.get(bookmark.spaceId) ?? null;
                                if (!accent) return undefined;
                                return {
                                  borderColor: withAlpha(accent, 0.55),
                                  backgroundColor: withAlpha(accent, 0.16),
                                  color: '#e2e8f0',
                                };
                              })()}
                            >
                              {bookmark.conversationTitle}
                            </span>
                          </p>
                          <p className="mt-1.5 text-[11px] text-white/60 line-clamp-2 break-words">
                            {bookmark.messagePreview}
                          </p>
                        </button>
                        <div className="px-3 pb-2.5">
                          <button
                            onClick={handleAsyncEvent(() => openSnippet(bookmark))}
                            className="text-[11px] inline-flex items-center gap-1 rounded-md border border-white/15 px-2 py-1 text-white/70 hover:text-white hover:border-cyan-300/40 transition-colors"
                          >
                            <ArrowUpRight className="w-3 h-3" />
                            Open in Chat
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>

                {selectedSnippet && (
                  <div className="rounded-xl border border-white/10 bg-[linear-gradient(155deg,rgba(34,211,238,0.08),rgba(0,0,0,0.25)_45%)] p-3">
                    <p className="text-[11px] text-cyan-100/70 uppercase tracking-wide">
                      Selected Reference
                    </p>
                    <p className="mt-1 text-xs text-white/80 line-clamp-2">
                      {selectedSnippet.title || selectedSnippet.conversationTitle}
                    </p>
                    <p className="mt-1 text-[11px] text-white/50">
                      {selectedSnippetCapture
                        ? `Captured in ${selectedSnippetCapture.noteTitle}.`
                        : 'Pending capture.'}
                    </p>
                    <button
                      onClick={() => navigate('/references')}
                      className="mt-2 w-full inline-flex items-center justify-center gap-1 text-[11px] rounded-md border border-white/20 px-2 py-1.5 text-white/70 hover:text-white hover:border-white/35 transition-colors"
                    >
                      <ArrowUpRight className="w-3 h-3" />
                      Manage in Reference Inbox
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {isLoading && conversations.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-white/40">
            <Loader2 className="w-5 h-5 animate-spin" />
          </div>
        ) : conversations.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 text-white/40 px-4 text-center">
            <MessageSquare className="w-8 h-8 mb-2 opacity-50" />
            <p className="text-sm">{isJournalScope ? 'No journal entries yet' : 'No conversations yet'}</p>
            <p className="text-xs mt-1">Start a new conversation above</p>
          </div>
        ) : (
          <nav
            className={`p-2 pr-3 ${isJournalScope ? 'space-y-3' : 'space-y-1'}`}
            role="navigation"
            aria-label="Conversations"
          >
            {journalConversationGroups.map((group) => (
              <section key={group.key} className={isJournalScope ? 'space-y-1.5' : 'space-y-1'}>
                {isJournalScope && group.label && (
                  <p className="px-2 text-[10px] uppercase tracking-wide text-emerald-100/55">
                    Notebook · {group.label}
                  </p>
                )}
                <div className="space-y-1">
                  {group.items.map((conversation, groupIndex) => {
                    const isActive = conversation.id === activeConversationId;
                    const isDeleting = deletingId === conversation.id;
                    const accent = conversation.spaceId
                      ? spaceAccentById.get(conversation.spaceId) ?? null
                      : null;
                    const conversationSpaceKind = conversation.spaceId
                      ? (spaceKindById.get(conversation.spaceId) ?? 'standard')
                      : 'standard';
                    const rowStyle = accent
                      ? {
                        borderColor: isActive ? withAlpha(accent, 0.65) : withAlpha(accent, 0.28),
                        background: isActive
                          ? `linear-gradient(90deg, ${withAlpha(accent, 0.2)} 0%, rgba(255,255,255,0.02) 100%)`
                          : withAlpha(accent, 0.08),
                        boxShadow: isActive ? `0 0 0 1px ${withAlpha(accent, 0.2)} inset` : undefined,
                      }
                      : undefined;

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
                        className={`group w-full text-left px-4 py-3 rounded-xl transition-all duration-300 cursor-pointer ${
                          isJournalScope
                            ? (isActive
                              ? 'bg-[linear-gradient(165deg,rgba(16,185,129,0.2),rgba(20,184,166,0.1))] shadow-lg shadow-emerald-900/25 border border-emerald-300/35'
                              : 'bg-[linear-gradient(165deg,rgba(16,185,129,0.09),rgba(15,23,42,0.15))] hover:bg-[linear-gradient(165deg,rgba(16,185,129,0.14),rgba(15,23,42,0.2))] border border-emerald-300/20')
                            : (isActive
                              ? 'bg-white/[0.08] backdrop-blur-xl shadow-lg shadow-blue-500/10 border border-blue-500/15'
                              : 'bg-white/[0.02] hover:bg-white/[0.04] border border-transparent hover:border-white/[0.06]')
                        }`}
                        style={rowStyle}
                      >
                        <div className="relative flex items-start gap-2">
                          <div className={`flex-1 min-w-0 transition-[padding] duration-200 ${isSelectionMode ? '' : 'pr-28'}`}>
                            {isJournalScope && (
                              <p className="mb-1 text-[10px] uppercase tracking-wide text-emerald-100/65">
                                Page {groupIndex + 1}
                              </p>
                            )}
                            <div className="flex items-center gap-2 mb-1">
                              {isSelectionMode && (
                                <input
                                  type="checkbox"
                                  checked={selectedConversationIds.has(conversation.id)}
                                  onChange={() => toggleConversationSelection(conversation.id)}
                                  onClick={(e) => e.stopPropagation()}
                                  className="h-3.5 w-3.5 rounded border-white/25 bg-white/10 accent-blue-400"
                                  aria-label={`Select conversation: ${conversation.title}`}
                                />
                              )}
                              {isJournalScope ? (
                                <NotebookPen className="w-4 h-4 text-emerald-100/80 flex-shrink-0" />
                              ) : (
                                <MessageSquare className="w-4 h-4 text-white/60 flex-shrink-0" />
                              )}
                              {renamingConversationId === conversation.id ? (
                                <div
                                  className="flex min-w-0 flex-1 items-center gap-1.5"
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
                                    className="h-7 min-w-0 flex-1 rounded-md border border-white/20 bg-black/25 px-2 text-xs text-white/90 placeholder:text-white/35 focus:outline-none focus:ring-1 focus:ring-blue-400/70"
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
                                    className="rounded-md border border-emerald-300/35 bg-emerald-500/15 p-1 text-emerald-100 transition-colors hover:border-emerald-300/60"
                                    aria-label="Save conversation title"
                                    title="Save title"
                                  >
                                    <Check className="h-3.5 w-3.5" />
                                  </button>
                                  <button
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      cancelRenameConversation();
                                    }}
                                    className="rounded-md border border-white/20 bg-white/5 p-1 text-white/70 transition-colors hover:border-white/35 hover:text-white"
                                    aria-label="Cancel rename"
                                    title="Cancel"
                                  >
                                    <X className="h-3.5 w-3.5" />
                                  </button>
                                </div>
                              ) : (
                                <h3
                                  className="text-sm font-medium text-white/90 break-words"
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
                              <p className={`text-[11px] mb-1 break-words ${isJournalScope ? 'text-emerald-100/65' : 'text-white/35'}`}>
                                <span
                                  className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border ${
                                    isJournalScope
                                      ? 'border-emerald-300/30 bg-emerald-500/10'
                                      : 'border-white/15 bg-white/5'
                                  }`}
                                  style={(() => {
                                    const spaceAccent = spaceAccentById.get(conversation.spaceId) ?? null;
                                    if (!spaceAccent) return undefined;
                                    return {
                                      borderColor: withAlpha(spaceAccent, 0.55),
                                      backgroundColor: withAlpha(spaceAccent, 0.16),
                                      color: '#e2e8f0',
                                    };
                                  })()}
                                >
                                  {spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}
                                  {conversationSpaceKind === 'journal' && (
                                    <span className="rounded-full border border-emerald-300/35 bg-emerald-500/15 px-1 py-0 text-[9px] uppercase tracking-wide text-emerald-100">
                                      Journal
                                    </span>
                                  )}
                                </span>
                              </p>
                            )}
                            <p className="text-xs text-white/40">
                              {(() => {
                                const date = new Date(conversation.updatedAt);
                                return isNaN(date.getTime()) ? 'Recently' : formatDistanceToNow(date, { addSuffix: true });
                              })()}
                            </p>
                          </div>

                          <div className={`${
                            isSelectionMode
                              ? 'hidden'
                              : isJournalScope
                                ? 'absolute right-0 top-0'
                                : 'absolute right-0 top-0 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto'
                          } transition-opacity duration-300 flex items-center gap-1`}>
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                beginRenameConversation(conversation.id, conversation.title);
                              }}
                              aria-label={`Rename conversation: ${conversation.title}`}
                              className="p-1.5 rounded-md bg-white/5 text-white/60 hover:text-cyan-200 transition-colors"
                              title="Rename conversation"
                            >
                              <Pencil className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationSaved(conversation.id, !conversation.isSaved);
                              })}
                              aria-label={`Save conversation: ${conversation.title}`}
                              className={`p-1.5 rounded-md transition-colors ${
                                conversation.isSaved
                                  ? 'bg-amber-500/20 text-amber-300'
                                  : 'bg-white/5 text-white/60 hover:text-amber-300'
                              }`}
                              title={conversation.isSaved ? 'Unsave' : 'Save'}
                            >
                              <Star className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationBookmarked(conversation.id, !conversation.isBookmarked);
                              })}
                              aria-label={`Bookmark conversation: ${conversation.title}`}
                              className={`p-1.5 rounded-md transition-colors ${
                                conversation.isBookmarked
                                  ? 'bg-sky-500/20 text-sky-300'
                                  : 'bg-white/5 text-white/60 hover:text-sky-300'
                              }`}
                              title={conversation.isBookmarked ? 'Remove bookmark' : 'Bookmark'}
                            >
                              <Bookmark className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationPinned(conversation.id, !conversation.isPinned);
                              })}
                              aria-label={`Pin conversation: ${conversation.title}`}
                              className={`p-1.5 rounded-md transition-colors ${
                                conversation.isPinned
                                  ? 'bg-violet-500/20 text-violet-300'
                                  : 'bg-white/5 text-white/60 hover:text-violet-300'
                              }`}
                              title={conversation.isPinned ? 'Unpin' : 'Pin'}
                            >
                              <Pin className="w-3.5 h-3.5" />
                            </button>

                            <button
                              onClick={handleAsyncEvent(async (e) => {
                                e.stopPropagation();
                                await setConversationArchived(conversation.id, !conversation.isArchived);
                              })}
                              aria-label={`${conversation.isArchived ? 'Unarchive' : 'Archive'} conversation: ${conversation.title}`}
                              className="p-1.5 rounded-md bg-white/5 text-white/60 hover:text-orange-300 transition-colors"
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
                              className="p-1.5 rounded-md bg-white/5 hover:bg-red-500/20 text-white/60 hover:text-red-400 disabled:opacity-50 transition-colors"
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
                          <p className={`text-xs mt-2 break-words ${
                            isJournalScope ? 'line-clamp-3 text-emerald-100/75' : 'line-clamp-2 text-white/50'
                          }`}>
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

      <div className="p-4 border-t border-white/10">
        <div className="text-xs text-white/40 text-center">
          {conversations.length} {isJournalScope ? 'entry' : 'conversation'}{conversations.length !== 1 ? 's' : ''}
        </div>
      </div>

      {isSpacesOpen && createPortal(
        <>
          <button
            type="button"
            aria-label="Close spaces panel"
            onClick={() => setIsSpacesOpen(false)}
            className={`fixed inset-0 bg-black/40 backdrop-blur-[1px] ${SPACES_MODAL_LAYER_CLASSES.backdrop}`}
          />
          <aside
            className={`fixed border-r border-white/10 bg-[#0b1118]/95 backdrop-blur-xl shadow-2xl shadow-black/40 ${SPACES_MODAL_LAYER_CLASSES.panel}`}
            style={spacesPanelFrame ?? undefined}
          >
            <div className={`${SPACES_MODAL_LAYER_CLASSES.content} flex h-full flex-col overflow-hidden`}>
              <div className={`flex items-center justify-between border-b border-white/10 px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <div>
                  <p className="text-[10px] uppercase tracking-wide text-white/40">Per-Space Context</p>
                  <h3 className="text-sm font-medium text-white/90">Spaces</h3>
                </div>
                <button
                  onClick={() => setIsSpacesOpen(false)}
                  className="rounded-md border border-white/15 bg-white/5 p-1.5 text-white/60 transition-colors hover:border-white/30 hover:text-white"
                  aria-label="Close spaces panel"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className={`flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setIsCreateSpaceOpen((open) => !open)}
                    className="inline-flex items-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-1.5 text-[11px] text-white/70 transition-colors hover:border-white/30 hover:text-white/90"
                  >
                    <FolderPlus className="w-3.5 h-3.5" />
                    {isCreateSpaceOpen ? 'Cancel' : 'New Space'}
                  </button>
                  <button
                    onClick={() => setIsSpaceEditorOpen((open) => !open)}
                    disabled={!selectedSpace}
                    className="inline-flex items-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-1.5 text-[11px] text-white/70 transition-colors hover:border-white/30 hover:text-white/90 disabled:cursor-not-allowed disabled:opacity-40"
                  >
                    <Settings2 className="w-3.5 h-3.5" />
                    Environment
                  </button>
                </div>

                {isCreateSpaceOpen && (
                  <div className="mt-2 space-y-2 rounded-lg border border-white/10 bg-white/[0.03] p-2.5">
                    <input
                      value={newSpaceNameDraft}
                      onChange={(e) => setNewSpaceNameDraft(e.target.value)}
                      placeholder={
                        newSpaceKindDraft === 'journal'
                          ? 'Journal name (e.g. Food Research, Weekly Notes)'
                          : 'Space name (e.g. Product, Research, Personal)'
                      }
                      className="w-full rounded-md border border-white/10 bg-white/[0.02] px-2.5 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                    />
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => setNewSpaceKindDraft('standard')}
                        className={`rounded-md border px-2 py-1 text-[11px] transition-colors ${
                          newSpaceKindDraft === 'standard'
                            ? 'border-blue-400/45 bg-blue-500/20 text-blue-100'
                            : 'border-white/15 bg-white/5 text-white/65 hover:border-white/30 hover:text-white'
                        }`}
                      >
                        Standard
                      </button>
                      <button
                        type="button"
                        onClick={() => setNewSpaceKindDraft('journal')}
                        className={`rounded-md border px-2 py-1 text-[11px] transition-colors ${
                          newSpaceKindDraft === 'journal'
                            ? 'border-emerald-400/45 bg-emerald-500/20 text-emerald-100'
                            : 'border-white/15 bg-white/5 text-white/65 hover:border-white/30 hover:text-white'
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
                        className="inline-flex items-center gap-1 rounded-md border border-white/15 px-2.5 py-1 text-[11px] text-white/65 transition-colors hover:border-white/30 hover:text-white"
                      >
                        Cancel
                      </button>
                      <button
                        onClick={handleAsyncEvent(createSpace)}
                        disabled={isCreatingSpace}
                        className="inline-flex items-center gap-1 rounded-md border border-blue-400/30 bg-blue-500/10 px-2.5 py-1 text-[11px] text-blue-200 transition-colors hover:border-blue-300/60 disabled:cursor-not-allowed disabled:opacity-50"
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
                  <p className="mb-2 text-[10px] uppercase tracking-wide text-white/40">
                    Space Selector
                  </p>
                  <div className="flex-1 min-h-0 space-y-1.5 overflow-y-auto pr-1">
                    <button
                      onClick={handleAsyncEvent(() => handleSpaceSelect(null))}
                      className={`w-full rounded-lg border px-2.5 py-2 text-left transition-colors ${
                        selectedSpaceId === null
                          ? 'border-blue-400/40 bg-blue-500/25 text-blue-100'
                          : 'border-white/10 bg-white/5 text-white/70 hover:border-white/25 hover:text-white'
                      }`}
                    >
                      <div className="flex items-start gap-2.5">
                        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-white/20 bg-white/10 text-sm">
                          <MessageSquare className="h-4 w-4" />
                        </div>
                        <div className="min-w-0">
                          <p className="truncate text-xs font-medium">All Spaces</p>
                          <p className="mt-0.5 text-[11px] text-white/55">
                            View every conversation
                          </p>
                        </div>
                      </div>
                    </button>

                    {standardSpaces.length > 0 && (
                      <p className="px-1 pt-1 text-[10px] uppercase tracking-wide text-white/45">
                        Spaces
                      </p>
                    )}

                    {standardSpaces.map((space) => {
                      const isSelected = selectedSpaceId === space.id;
                      const accent = normalizeHexColor(space.accentColor);
                      const cardStyle = accent
                        ? {
                          borderColor: withAlpha(accent, isSelected ? 0.65 : 0.35),
                          backgroundColor: withAlpha(accent, isSelected ? 0.2 : 0.08),
                          boxShadow: isSelected
                            ? `inset 0 0 0 1px ${withAlpha(accent, 0.22)}`
                            : undefined,
                        }
                        : undefined;
                      const iconStyle = accent
                        ? {
                          borderColor: withAlpha(accent, 0.55),
                          backgroundColor: withAlpha(accent, 0.2),
                        }
                        : undefined;

                      return (
                        <button
                          key={space.id}
                          onClick={handleAsyncEvent(() => handleSpaceSelect(space.id))}
                          className={`w-full rounded-lg border px-2.5 py-2 text-left transition-colors ${
                            accent
                              ? 'text-white/85 hover:text-white'
                              : isSelected
                                ? 'border-blue-400/40 bg-blue-500/25 text-blue-100'
                                : 'border-white/10 bg-white/5 text-white/70 hover:border-white/25 hover:text-white'
                          }`}
                          style={cardStyle}
                          title={space.description ?? space.name}
                        >
                          <div className="flex items-start gap-2.5">
                            <div
                              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-white/20 bg-white/10 text-sm"
                              style={iconStyle}
                            >
                              {space.icon || <MessageSquare className="h-4 w-4" />}
                            </div>
                            <div className="min-w-0">
                              <p className="truncate text-xs font-medium">{space.name}</p>
                              {space.description ? (
                                <p className="mt-0.5 line-clamp-2 text-[11px] text-white/55">
                                  {space.description}
                                </p>
                              ) : (
                                <p className="mt-0.5 text-[11px] text-white/45">
                                  Custom space context
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
                  <div className="mt-2 space-y-2 rounded-lg border border-white/10 bg-white/[0.03] p-3">
                    <p className="text-[11px] uppercase tracking-wide text-white/45">
                      Space Environment
                    </p>

                    <details open className="rounded-md border border-white/10 bg-white/[0.02] p-2">
                      <summary className="cursor-pointer text-[11px] text-white/70">Basics</summary>
                      <div className="mt-2 space-y-2">
                        <div className="grid grid-cols-[72px_1fr] gap-2">
                          <input
                            value={spaceIconDraft}
                            onChange={(e) => setSpaceIconDraft(e.target.value)}
                            placeholder="Icon"
                            className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                          />
                          <input
                            value={spaceNameDraft}
                            onChange={(e) => setSpaceNameDraft(e.target.value)}
                            placeholder="Space name"
                            className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                          />
                        </div>
                        <div className="grid grid-cols-[92px_1fr_56px] gap-2">
                          <input
                            type="color"
                            value={normalizeHexColor(spaceAccentDraft) ?? '#3b82f6'}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            className="h-8 w-full rounded-md border border-white/10 bg-white/[0.03] p-1"
                            title="Accent color"
                          />
                          <input
                            value={spaceAccentDraft}
                            onChange={(e) => setSpaceAccentDraft(e.target.value)}
                            placeholder="#3b82f6"
                            className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                          />
                          <button
                            type="button"
                            onClick={() => setSpaceAccentDraft('')}
                            className="rounded-md border border-white/15 bg-white/5 px-2 py-1 text-[11px] text-white/65 transition-colors hover:border-white/30 hover:text-white"
                          >
                            Clear
                          </button>
                        </div>
                        <input
                          value={spaceDescriptionDraft}
                          onChange={(e) => setSpaceDescriptionDraft(e.target.value)}
                          placeholder="Description"
                          className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                        />
                      </div>
                    </details>

                    <details className="rounded-md border border-white/10 bg-white/[0.02] p-2">
                      <summary className="cursor-pointer text-[11px] text-white/70">AI Defaults</summary>
                      <div className="mt-2 space-y-2">
                        <input
                          value={spaceModelDraft}
                          onChange={(e) => setSpaceModelDraft(e.target.value)}
                          placeholder="Default model id (optional)"
                          list="space-model-options"
                          className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                        />
                        <datalist id="space-model-options">
                          {availableSpaceModels.map((modelId) => (
                            <option key={modelId} value={modelId} />
                          ))}
                        </datalist>
                        <textarea
                          value={spacePromptDraft}
                          onChange={(e) => setSpacePromptDraft(e.target.value)}
                          placeholder="Space prompt (prepended for this environment)"
                          rows={4}
                          className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60 resize-y"
                        />
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() => setSpaceKbDefault((value) => !value)}
                            className={`px-2 py-1 text-[11px] rounded-md border transition-colors ${
                              spaceKbDefault
                                ? 'bg-blue-500/20 border-blue-400/45 text-blue-100'
                                : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
                            }`}
                          >
                            KB Default
                          </button>
                          <button
                            type="button"
                            onClick={() => setSpaceWebDefault((value) => !value)}
                            className={`px-2 py-1 text-[11px] rounded-md border transition-colors ${
                              spaceWebDefault
                                ? 'bg-blue-500/20 border-blue-400/45 text-blue-100'
                                : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
                            }`}
                          >
                            Web Default
                          </button>
                          <button
                            type="button"
                            onClick={toggleSpaceDeepResearchDefault}
                            className={`px-2 py-1 text-[11px] rounded-md border transition-colors ${
                              spaceDeepResearchDefault
                                ? 'bg-blue-500/20 border-blue-400/45 text-blue-100'
                                : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
                            }`}
                          >
                            Deep Research
                          </button>
                        </div>
                        {spaceDeepResearchDefault && (
                          <div
                            role="status"
                            aria-live="polite"
                            className="rounded-md border border-amber-400/30 bg-amber-500/10 px-2.5 py-2 text-[11px] text-amber-100/90"
                          >
                            Deep Research default is on for this space, so responses can take longer.
                          </div>
                        )}
                      </div>
                    </details>

                    <div className="flex items-center gap-1.5">
                      <button
                        onClick={handleAsyncEvent(saveSpaceEnvironment)}
                        disabled={isSavingSpace}
                        className="inline-flex items-center gap-1 px-2.5 py-1 text-[11px] rounded-md border border-emerald-400/40 text-emerald-200 hover:border-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      >
                        {isSavingSpace ? (
                          <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        ) : (
                          <Save className="w-3.5 h-3.5" />
                        )}
                        Save Space
                      </button>
                      {selectedSpace.id !== 'space_general' && (
                        <button
                          onClick={handleAsyncEvent(() => setSelectedSpaceArchived(!selectedSpace.isArchived))}
                          disabled={isArchivingSpace}
                          className="inline-flex items-center gap-1 px-2.5 py-1 text-[11px] rounded-md border border-orange-400/35 text-orange-200 hover:border-orange-300 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                        >
                          {isArchivingSpace ? (
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          ) : (
                            <Archive className="w-3.5 h-3.5" />
                          )}
                          {selectedSpace.isArchived ? 'Restore Space' : 'Archive Space'}
                        </button>
                      )}
                    </div>

                    {archivedSpaces.length > 0 && (
                      <details className="rounded-md border border-white/10 bg-white/[0.02] p-2">
                        <summary className="cursor-pointer text-[11px] text-white/60">
                          Archived Spaces ({archivedSpaces.length})
                        </summary>
                        <div className="mt-2 space-y-1">
                          {archivedSpaces.slice(0, 6).map((space) => (
                            <div
                              key={space.id}
                              className="flex items-center justify-between rounded-md border border-white/10 bg-white/[0.02] px-2 py-1"
                            >
                              <span className="text-[11px] text-white/70 truncate">
                                {space.icon ? `${space.icon} ` : ''}{space.name}
                              </span>
                              <button
                                onClick={handleAsyncEvent(() => restoreArchivedSpace(space.id))}
                                disabled={isRestoringSpace}
                                className="text-[11px] rounded border border-white/20 px-1.5 py-0.5 text-white/70 hover:text-white hover:border-white/35 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
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

              <div className={`border-t border-white/10 px-4 py-3 ${SPACES_MODAL_LAYER_CLASSES.section}`}>
                <p className="text-xs text-white/45 text-center">
                  {activeSpacesOrdered.length} space{activeSpacesOrdered.length !== 1 ? 's' : ''}
                </p>
              </div>
            </div>
          </aside>
        </>,
        document.body
      )}
    </div>
  );
}
