import { useEffect, useMemo, useState } from 'react';

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
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { useDebounce } from '../../hooks/useDebounce';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { useDownloadedModelsStore } from '../../stores/downloadedModelsStore';
import {
  buildCapturedChatReferenceIndex,
  chatReferenceKey,
  type CapturedChatReference,
} from '../../utils/chatReferenceIndex';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

import type { ConversationMessageBookmarkDto } from '../../types';

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
    deleteConversation,
    clearError,
  } = useConversationsStore();

  const [isCreating, setIsCreating] = useState(false);
  const [isSpacesOpen, setIsSpacesOpen] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
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
  const spaceAccentById = useMemo(() => {
    const map = new Map<string, string | null>();
    for (const space of spaces) {
      map.set(space.id, normalizeHexColor(space.accentColor));
    }
    return map;
  }, [spaces]);
  const selectedSpace = useMemo(() => {
    if (!selectedSpaceId) return null;
    return spaces.find((space) => space.id === selectedSpaceId) ?? null;
  }, [selectedSpaceId, spaces]);

  const archivedSpaces = useMemo(
    () => spaces.filter((space) => space.isArchived),
    [spaces]
  );
  const activeSpacesOrdered = useMemo(
    () => spaces.filter((space) => !space.isArchived),
    [spaces]
  );

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

  useEffect(() => {
    setLocalQuery(searchQuery);
  }, [searchQuery]);

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
      const title = `Conversation ${new Date().toLocaleDateString()}`;
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
  }> = [
    { id: 'all', label: 'All' },
    { id: 'saved', label: 'Saved' },
    { id: 'bookmarked', label: 'Bookmarked' },
    { id: 'pinned', label: 'Pinned' },
    { id: 'snippets', label: 'References' },
    { id: 'archived', label: 'Archived' },
  ];

  const openSnippet = async (bookmark: ConversationMessageBookmarkDto) => {
    await selectConversation(bookmark.conversationId);
    scrollToMessage(bookmark.messageId);
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
      const existingNames = new Set(spaces.map((space) => space.name.toLowerCase()));

      let name = requestedName;
      if (!name) {
        let idx = spaces.length + 1;
        name = `Space ${idx}`;
        while (existingNames.has(name.toLowerCase())) {
          idx += 1;
          name = `Space ${idx}`;
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

      const result = await VaultAPI.createConversationSpace({
        name,
        description: null,
        icon: null,
        accentColor: null,
        spacePrompt: null,
        defaultModelName: null,
        toolPreferencesJson: JSON.stringify({
          knowledgeBase: false,
          webSearch: false,
          deepResearchMode: false,
          enabledTools: [],
        }),
      });
      if (!result.ok) {
        return;
      }

      await useConversationsStore.getState().loadSpaces();
      setSelectedSpace(result.data.id);
      await loadConversations({ spaceId: result.data.id });
      setIsCreateSpaceOpen(false);
      setNewSpaceNameDraft('');
      setIsSpaceEditorOpen(true);
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
        toolPreferencesJson: JSON.stringify({
          knowledgeBase: spaceKbDefault,
          webSearch: spaceWebDefault,
          deepResearchMode: spaceDeepResearchDefault,
          enabledTools: spaceWebDefault ? ['web_search', 'fetch_url_content'] : [],
        }),
      };

      const result = await VaultAPI.updateConversationSpace(payload);
      if (!result.ok) {
        return;
      }

      await useConversationsStore.getState().loadSpaces();
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
          useConversationsStore.getState().loadSpaces(),
          loadConversations({ spaceId: null }),
        ]);
      } else {
        await useConversationsStore.getState().loadSpaces();
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
      await useConversationsStore.getState().loadSpaces();
    } finally {
      setIsRestoringSpace(false);
    }
  };

  const selectedSnippetCapture = selectedSnippet
    ? getCapturedSnippetReference(selectedSnippet)
    : null;

  return (
    <div
      className={`relative w-80 h-full bg-black/30 backdrop-blur-2xl border-r border-white/[0.06] flex flex-col ${
        isSpacesOpen ? SPACES_MODAL_LAYER_CLASSES.root : ''
      }`}
    >
      <div className="p-4 border-b border-white/10">
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
          <span>New Conversation</span>
        </button>

        <div className="mt-3 flex items-center justify-between gap-2 rounded-lg border border-white/10 bg-white/[0.02] px-3 py-2">
          <div className="min-w-0">
            <p className="text-[10px] uppercase tracking-wide text-white/45">Space Scope</p>
            <p className="truncate text-xs text-white/80">
              {selectedSpace
                ? `${selectedSpace.icon ? `${selectedSpace.icon} ` : ''}${selectedSpace.name}`
                : 'All Spaces'}
            </p>
          </div>
          <button
            onClick={() => setIsSpacesOpen(true)}
            className="inline-flex items-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-1.5 text-[11px] text-white/70 transition-colors hover:border-white/30 hover:text-white/90"
          >
            <Settings2 className="w-3.5 h-3.5" />
            Spaces
          </button>
        </div>

        <div className="mt-3 flex items-center gap-1.5 overflow-x-auto pb-1">
          {filterOptions.map((option) => (
            <button
              key={option.id}
              onClick={handleAsyncEvent(() => handleFilterSelect(option.id))}
              className={`px-2.5 py-1 text-xs rounded-md border transition-colors whitespace-nowrap ${
                filterMode === option.id
                  ? 'bg-white/15 border-white/30 text-white'
                  : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>

        <div className="mt-3 relative">
          <Search className="w-4 h-4 text-white/40 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            value={localQuery}
            onChange={(e) => setLocalQuery(e.target.value)}
            placeholder="Search conversations..."
            className="w-full pl-9 pr-3 py-2.5 rounded-lg border border-white/10 bg-white/[0.03] text-sm text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
          />
        </div>
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

      <div className="flex-1 overflow-y-auto">
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
                          <p className="mt-1.5 text-[11px] text-white/60 line-clamp-2 break-all">
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
            <p className="text-sm">No conversations yet</p>
            <p className="text-xs mt-1">Start a new conversation above</p>
          </div>
        ) : (
          <nav className="p-2 space-y-1" role="navigation" aria-label="Conversations">
            {conversations.map((conversation) => {
              const isActive = conversation.id === activeConversationId;
              const isDeleting = deletingId === conversation.id;
              const accent = conversation.spaceId
                ? spaceAccentById.get(conversation.spaceId) ?? null
                : null;
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
                  onClick={handleAsyncEvent(() => selectConversation(conversation.id))}
                  role="button"
                  tabIndex={0}
                  onKeyDown={handleAsyncEvent(async (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      await selectConversation(conversation.id);
                    }
                  })}
                  aria-label={`Select conversation: ${conversation.title}`}
                  aria-current={isActive ? 'page' : undefined}
                  className={`group w-full text-left px-4 py-3 rounded-xl transition-all duration-300 cursor-pointer ${
                    isActive
                      ? 'bg-white/[0.08] backdrop-blur-xl shadow-lg shadow-blue-500/10 border border-blue-500/15'
                      : 'bg-white/[0.02] hover:bg-white/[0.04] border border-transparent hover:border-white/[0.06]'
                  }`}
                  style={rowStyle}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <MessageSquare className="w-4 h-4 text-white/60 flex-shrink-0" />
                        <h3 className="text-sm font-medium text-white/90 truncate">
                          {conversation.title}
                        </h3>
                      </div>
                      {conversation.spaceId && (
                        <p className="text-[11px] text-white/35 mb-1 truncate">
                          <span
                            className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-white/15 bg-white/5"
                            style={(() => {
                              const accent = spaceAccentById.get(conversation.spaceId) ?? null;
                              if (!accent) return undefined;
                              return {
                                borderColor: withAlpha(accent, 0.55),
                                backgroundColor: withAlpha(accent, 0.16),
                                color: '#e2e8f0',
                              };
                            })()}
                          >
                            {spaceNameById.get(conversation.spaceId) ?? conversation.spaceId}
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

                    <div className="opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity duration-300 flex items-center gap-1">
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
                    <p className="text-xs text-white/50 mt-2 line-clamp-2 break-all">
                      {conversation.lastMessagePreview ?? conversation.messages?.[conversation.messages.length - 1]?.content}
                    </p>
                  )}
                </div>
              );
            })}
          </nav>
        )}
      </div>

      <div className="p-4 border-t border-white/10">
        <div className="text-xs text-white/40 text-center">
          {conversations.length} conversation{conversations.length !== 1 ? 's' : ''}
        </div>
      </div>

      {isSpacesOpen && (
        <>
          <button
            type="button"
            aria-label="Close spaces panel"
            onClick={() => setIsSpacesOpen(false)}
            className={`absolute inset-0 bg-black/40 backdrop-blur-[1px] ${SPACES_MODAL_LAYER_CLASSES.backdrop}`}
          />
          <aside className={`absolute left-full top-0 h-full w-[360px] border-r border-white/10 bg-[#0b1118]/95 backdrop-blur-xl shadow-2xl shadow-black/40 ${SPACES_MODAL_LAYER_CLASSES.panel}`}>
            <div className={`${SPACES_MODAL_LAYER_CLASSES.content} flex h-full flex-col`}>
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
                      placeholder="Space name (e.g. Product, Research, Personal)"
                      className="w-full rounded-md border border-white/10 bg-white/[0.02] px-2.5 py-1.5 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-blue-400/60"
                    />
                    <div className="flex items-center justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => {
                          setIsCreateSpaceOpen(false);
                          setNewSpaceNameDraft('');
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

                    {activeSpacesOrdered.map((space) => {
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
                            onClick={() => setSpaceDeepResearchDefault((value) => !value)}
                            className={`px-2 py-1 text-[11px] rounded-md border transition-colors ${
                              spaceDeepResearchDefault
                                ? 'bg-blue-500/20 border-blue-400/45 text-blue-100'
                                : 'bg-white/5 border-white/10 text-white/60 hover:text-white/90'
                            }`}
                          >
                            Deep Research
                          </button>
                        </div>
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
        </>
      )}
    </div>
  );
}
