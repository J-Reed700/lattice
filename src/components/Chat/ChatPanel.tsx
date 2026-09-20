import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { useQueryClient } from '@tanstack/react-query';
import { motion, useReducedMotion } from 'framer-motion';
import { ArrowUp, ChevronDown, Cpu, FileText, GitBranch, Library, MessageCircle, Paperclip, RefreshCw, Scissors, ScrollText, Settings2, Square } from 'lucide-react';

import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';

import { ChatDropStaging } from './ChatDropStaging';
import { ChatEmptyStateIngestDelta } from './ChatEmptyStateIngestDelta';
import { ChatModelNotice } from './ChatModelNotice';
import { ChatStarters } from './ChatStarters';
import { ComposerSuggest } from './composer/ComposerSuggest';
import { describeFocus, FocusChips } from './composer/FocusChips';
import { ModeChips } from './composer/ModeChips';
import { matchSlashCommands, parseSlashSubmission } from './composer/slashCommands';
import { replaceTrigger } from './composer/suggestTrigger';
import { useComposerSuggest } from './composer/useComposerSuggest';
import { useSpaceDocuments } from './composer/useSpaceDocuments';
import { ComposerControls, WEB_TOOL_NAMES, WIKI_TOOL_NAMES, DEEP_RESEARCH_WARNING_MESSAGE } from './ComposerControls';
import { ConversationLinkedDocumentsPanel } from './ConversationLinkedDocumentsPanel';
import { ConversationMemoryPanel } from './ConversationMemoryPanel';
import { ImportFailuresNotice } from './ImportFailuresNotice';
import { Message } from './Message';
import { ModelPickerPopover } from './ModelPickerPopover';
import { GENERAL_SPACE_ID, SpacePickerPopover, useOpenSpaces } from './SpacePickerPopover';
import { useChatFileDrop } from './useChatFileDrop';
import { UtilityModelNotice } from './UtilityModelNotice';
import { useSettingsQuery } from '../../hooks/queries/useSettingsQuery';
import { conversationKeys } from '../../hooks/useConversationsController';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { selectIsChatWarming, useModelWarmupStore } from '../../stores/modelWarmupStore';
import { toast } from '../../stores/toastStore';
import { resolveChatModel } from '../../utils/chatModelSelection';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';

import type { CompactionRecord, CustomToolSettings, SpaceDocument, ToolPreferences } from '../../types';
import type { SuggestItem } from './composer/ComposerSuggest';
import type { ModeChipId } from './composer/ModeChips';
import type { ComposerModeState, SlashCommandId } from './composer/slashCommands';
import type { SuggestTrigger } from './composer/suggestTrigger';


/** Poll a batch job until it stops moving, or five minutes elapse. */
const BATCH_POLL_INTERVAL_MS = 1000;
const BATCH_POLL_TIMEOUT_MS = 5 * 60 * 1000;

/** The textarea points at the suggestion list through this, for screen readers. */
const SUGGEST_LIST_ID = 'composer-suggest-list';

type TurnMode = 'auto' | 'followup' | 'query';

const normalizeEnabledTools = (value: unknown): string[] | undefined => {
  if (!Array.isArray(value)) {
    return undefined;
  }

  return [...new Set(value.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean))];
};

const setToolNames = (
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

const defaultToolPreferences = (): ToolPreferences => ({
  knowledgeBase: false,
  webSearch: false,
  deepResearchMode: false,
  followupMode: false,
  turnMode: 'auto',
  enabledTools: [],
});

const normalizeTurnMode = (value: unknown): TurnMode | null => {
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

const parseToolPreferences = (serialized: string | null | undefined): ToolPreferences => {
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

export function ChatPanel() {
  const { activeModel, downloadedModels, setActiveChatModel } = useDownloadedModels();
  const {
    activeConversationId,
    conversations,
    spaces,
    selectedSpaceId,
    inFlightGenerations,
    sendMessage,
    cancelGeneration,
    createConversation,
    optimisticMessages,
    messageRetrieval,
    composerDraft,
    setComposerDraft,
    regenerateResponse,
    forkConversation,
    compactConversation,
    moveConversationToSpace,
    loadConversationLinkedDocuments,
  } = useConversationsStore();
  const queryClient = useQueryClient();
  const settings = useSettingsQuery().data;

  const isSending = activeConversationId
    ? inFlightGenerations.has(activeConversationId)
    : false;

  const [input, setInput] = useState('');
  const [toolPreferences, setToolPreferences] = useState<ToolPreferences>(defaultToolPreferences);
  const [customTools, setCustomTools] = useState<CustomToolSettings[]>([]);
  const [isControlsOpen, setIsControlsOpen] = useState(false);
  const [isCreatingConversation, setIsCreatingConversation] = useState(false);
  const [isImportingFiles, setIsImportingFiles] = useState(false);
  const [isCompacting, setIsCompacting] = useState(false);
  // The memory reader is an overlay off the palette, like the source reader:
  // a read-only look at what was recorded, never a place to edit it.
  const [isMemoryOpen, setIsMemoryOpen] = useState(false);
  const [focusDocuments, setFocusDocuments] = useState<SpaceDocument[]>([]);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [compactionByConversation, setCompactionByConversation] = useState<
    Record<string, CompactionRecord>
  >({});
  const panelRef = useRef<HTMLDivElement>(null);
  const modelLabelRef = useRef<HTMLButtonElement>(null);
  const toolPreferencesRef = useRef<ToolPreferences>(toolPreferences);
  const lastAppliedToolPreferenceConversationRef = useRef<string | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const previousConversationIdRef = useRef<string | null>(null);
  const shouldAutoScrollRef = useRef(true);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const knownMessageIdsRef = useRef<{ conversationId: string | null; ids: Set<string> }>({
    conversationId: null,
    ids: new Set(),
  });
  const prefersReducedMotion = useReducedMotion();

  const enabledToolSet = useMemo(
    () => new Set(toolPreferences.enabledTools ?? []),
    [toolPreferences.enabledTools]
  );

  const messages = useMemo(() => {
    if (!activeConversationId) return [];

    const realMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    const optimisticForConversation = Array.from(optimisticMessages.values()).filter(
      (message) =>
        message.conversationId === activeConversationId || message.conversationId === 'temp'
    );
    return [...realMessages, ...optimisticForConversation];
  }, [activeConversationId, conversations, optimisticMessages]);

  /**
   * Raw content of the loaded messages, for resolving memory evidence spans.
   *
   * The memory panel needs the stored text, not the rendered markdown: its
   * spans are UTF-8 byte offsets into what the backend holds.
   */
  const messageContentById = useMemo(() => {
    const byId = new Map<string, string>();
    for (const message of messages) {
      if ('id' in message && typeof message.content === 'string') {
        byId.set(message.id, message.content);
      }
    }
    return byId;
  }, [messages]);

  const getMessageKey = useCallback(
    (message: typeof messages[number]): string =>
      'tempId' in message ? message.tempId : message.id,
    []
  );

  const freshMessageKeys = useMemo(() => {
    const fresh = new Set<string>();
    const tracker = knownMessageIdsRef.current;
    if (tracker.conversationId !== activeConversationId) {
      tracker.conversationId = activeConversationId ?? null;
      tracker.ids = new Set(messages.map(getMessageKey));
      return fresh;
    }
    for (const message of messages) {
      const key = getMessageKey(message);
      if (!tracker.ids.has(key)) {
        fresh.add(key);
        tracker.ids.add(key);
      }
    }
    return fresh;
  }, [activeConversationId, getMessageKey, messages]);

  useEffect(() => {
    const loadCustomTools = async () => {
      const result = await VaultAPI.getSettings();
      if (!result.ok) {
        return;
      }

      const configured = (result.data.llm.customTools ?? [])
        .filter((tool) => tool.enabled && tool.name.trim().length > 0)
        .sort((a, b) => a.name.localeCompare(b.name));
      setCustomTools(configured);
    };

    void loadCustomTools();
  }, []);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const previousConversationId = previousConversationIdRef.current;
    const isConversationChange = previousConversationId !== activeConversationId;
    previousConversationIdRef.current = activeConversationId ?? null;

    if (isConversationChange) {
      shouldAutoScrollRef.current = true;
    } else if (!shouldAutoScrollRef.current) {
      return;
    }

    // The global `prefers-reduced-motion` rule in index.css covers CSS
    // transitions, not programmatic scrolling: this ride has to be opted out
    // of here or it happens on every message.
    container.scrollTo({
      top: container.scrollHeight,
      behavior: isConversationChange || prefersReducedMotion ? 'auto' : 'smooth',
    });
  }, [messages, activeConversationId, prefersReducedMotion]);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${textareaRef.current.scrollHeight}px`;
    }
  }, [input]);

  useEffect(() => {
    if (!activeConversationId) {
      lastAppliedToolPreferenceConversationRef.current = null;
      const defaults = defaultToolPreferences();
      setToolPreferences(defaults);
      toolPreferencesRef.current = defaults;
      return;
    }
    if (lastAppliedToolPreferenceConversationRef.current === activeConversationId) {
      return;
    }

    const activeConversation = conversations.find(
      (conversation) => conversation.id === activeConversationId
    );
    if (!activeConversation) {
      return;
    }

    const spaceId = activeConversation?.spaceId;
    if (!spaceId) {
      const defaults = defaultToolPreferences();
      setToolPreferences(defaults);
      toolPreferencesRef.current = defaults;
      lastAppliedToolPreferenceConversationRef.current = activeConversationId;
      return;
    }

    const space = spaces.find((item) => item.id === spaceId);
    if (!space) {
      return;
    }

    const next = parseToolPreferences(space.toolPreferencesJson);
    setToolPreferences(next);
    toolPreferencesRef.current = next;

    lastAppliedToolPreferenceConversationRef.current = activeConversationId;
  }, [activeConversationId, conversations, spaces]);

  const updateToolPreferences = useCallback(
    (updater: (_prev: ToolPreferences) => ToolPreferences) => {
      setToolPreferences((prev) => {
        const next = updater(prev);
        toolPreferencesRef.current = next;
        return next;
      });
    },
    []
  );

  /**
   * The chips above the composer and the ids the turn goes out with, set
   * together. The names are only here to draw the chips; the ids are what the
   * backend intersects with this conversation's space.
   */
  const applyFocusDocuments = useCallback(
    (next: SpaceDocument[]) => {
      setFocusDocuments(next);
      updateToolPreferences((prev) => ({
        ...prev,
        focusDocumentIds: next.map((document) => document.documentId),
      }));
    },
    [updateToolPreferences]
  );

  // Focus belongs to the chat it was set in. Opening another one starts over.
  useEffect(() => {
    applyFocusDocuments([]);
  }, [activeConversationId, applyFocusDocuments]);

  const toggleKnowledgeBase = () => {
    updateToolPreferences((prev) => ({
      ...prev,
      knowledgeBase: !prev.knowledgeBase,
    }));
  };

  const toggleWebTools = () => {
    updateToolPreferences((prev) => {
      const enabled = !prev.webSearch;
      return {
        ...prev,
        webSearch: enabled,
        enabledTools: setToolNames(prev.enabledTools, WEB_TOOL_NAMES, enabled),
      };
    });
  };

  const toggleWikiTools = () => {
    updateToolPreferences((prev) => {
      const currentlyEnabled = WIKI_TOOL_NAMES.some((toolName) =>
        (prev.enabledTools ?? []).includes(toolName)
      );
      return {
        ...prev,
        enabledTools: setToolNames(prev.enabledTools, WIKI_TOOL_NAMES, !currentlyEnabled),
      };
    });
  };

  const toggleDeepResearch = () => {
    const enabling = !(toolPreferencesRef.current.deepResearchMode ?? false);
    updateToolPreferences((prev) => ({
      ...prev,
      deepResearchMode: !(prev.deepResearchMode ?? false),
    }));
    if (enabling) {
      toast.warning('Deep research enabled', {
        message: DEEP_RESEARCH_WARNING_MESSAGE,
        duration: 5000,
      });
    }
  };

  const toggleCustomTool = (toolName: string) => {
    updateToolPreferences((prev) => {
      const currentlyEnabled = (prev.enabledTools ?? []).includes(toolName);
      return {
        ...prev,
        enabledTools: setToolNames(prev.enabledTools, [toolName], !currentlyEnabled),
      };
    });
  };

  const handleTurnModeChange = (nextMode: TurnMode) => {
    updateToolPreferences((prev) => ({
      ...prev,
      turnMode: nextMode,
      followupMode: nextMode === 'followup',
    }));
  };

  // The applied compaction for the active conversation, if any, drives the
  // "Context compacted" divider. Local session state (from a /compact run in
  // this session) takes precedence; otherwise fall back to the persisted
  // record on the conversation so the divider survives a reload.
  const compactionRecord = activeConversationId
    ? compactionByConversation[activeConversationId] ??
      conversations.find((conversation) => conversation.id === activeConversationId)
        ?.compaction ??
      null
    : null;

  const handleCompact = useCallback(
    async (conversationId: string) => {
      if (isCompacting) return;
      setIsCompacting(true);
      // Summarizing runs a model call, so it can take a while with nothing else
      // on screen to show for it.
      toast.info('Compacting context', {
        message: 'Summarizing the older messages…',
      });
      try {
        const record = await compactConversation(conversationId);
        if (record) {
          setCompactionByConversation((prev) => ({
            ...prev,
            [conversationId]: record,
          }));
          // No "lossless"/"complete" framing: compaction summarizes, and the
          // only thing it actually guarantees is that active requirements
          // survive and the raw messages are still searchable.
          toast.success('Context compacted', {
            message:
              'Older context compacted. Active requirements preserved; original messages remain searchable.',
          });
        }
      } finally {
        setIsCompacting(false);
      }
    },
    [compactConversation, isCompacting]
  );

  /**
   * What a turn actually goes out with. Query mode turns every source on; the
   * focus documents ride along with everything else the composer is showing,
   * so a regenerated turn is the turn the chips describe.
   */
  const resolveEffectiveToolPreferences = useCallback((): ToolPreferences => {
    const current = toolPreferencesRef.current;
    const mode =
      normalizeTurnMode(current.turnMode) ?? (current.followupMode ? 'followup' : 'auto');
    const queryModeEnabledTools =
      mode === 'query'
        ? [
            ...new Set([
              ...(current.enabledTools ?? []),
              ...WEB_TOOL_NAMES,
              ...customTools.map((tool) => tool.name),
            ]),
          ]
        : current.enabledTools;

    return {
      ...current,
      turnMode: mode,
      followupMode: mode === 'followup',
      knowledgeBase: mode === 'query' ? true : current.knowledgeBase,
      webSearch: mode === 'query' ? true : current.webSearch,
      enabledTools: queryModeEnabledTools,
    };
  }, [customTools]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isSending || !activeConversationId) return;
    // Block submit while warming up or before chat model downloads.
    if (useModelWarmupStore.getState().chat.phase === 'started') return;
    if (isChatUnavailable) return;

    const message = input.trim();
    setInput('');

    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }

    // A message that is nothing but a command runs the command instead of being
    // sent. /compact was the first of these — a bare regex here that nothing on
    // screen mentioned — and every command in the menu now comes through the
    // same door, typed or picked.
    const typedCommand = parseSlashSubmission(message);
    if (typedCommand) {
      runSlashCommand(typedCommand);
      return;
    }

    await sendMessage(message, activeConversationId, resolveEffectiveToolPreferences());
  };

  const handleCancel = async () => {
    if (!activeConversationId || !isSending) return;
    await cancelGeneration(activeConversationId);
  };

  const activeConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [conversations, activeConversationId]
  );
  const conversationSpaceId = activeConversation?.spaceId ?? null;
  const isScopedToLinkedFiles = Boolean(
    conversationSpaceId && conversationSpaceId !== GENERAL_SPACE_ID
  );
  /**
   * Which space the empty state's suggested questions come from. The chat being
   * shown decides; before one exists, the sidebar's selection is the next best
   * answer, because that is the space the first message will land in.
   */
  const startersSpaceId = conversationSpaceId ?? selectedSpaceId ?? null;

  const {
    isDragging,
    staged,
    add: addStagedPaths,
    clear: clearStaged,
    remove: removeStaged,
  } = useChatFileDrop(panelRef, () => {
    // Staging is the hook's own state; the panel only needs to re-render.
  });

  /** The palette's way in, for people who would rather not drag. */
  const handleChooseFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({ multiple: true });
      if (!picked) return;
      addStagedPaths(Array.isArray(picked) ? picked : [picked]);
    } catch {
      // Not in a Tauri webview: the drop target is still there.
    }
  }, [addStagedPaths]);

  const openSpaces = useOpenSpaces();
  const conversationSpaceName =
    spaces.find((space) => space.id === conversationSpaceId)?.name ?? null;
  // With General alone there is nothing to choose and nothing to explain.
  const showSpacePicker = Boolean(
    activeConversationId && conversationSpaceName && (openSpaces.length > 1 || isScopedToLinkedFiles)
  );

  /**
   * Move this chat to another space, which changes the documents every later
   * answer can draw on. The toast names the space: General is a space like any
   * other, and calling a move there "searching your whole vault" promised
   * documents filed elsewhere that it cannot reach.
   */
  const handleChangeSpace = useCallback(
    async (spaceId: string, spaceName: string) => {
      if (!activeConversationId) return;
      // A document pinned in the old space is not in the new one, and focus
      // fails closed: keeping it would leave the chat asking nothing at all.
      applyFocusDocuments([]);
      await moveConversationToSpace(activeConversationId, spaceId);
      toast.success(`Now searching ${spaceName}`, {
        message: 'Answers from here on use the documents filed there.',
      });
    },
    [activeConversationId, applyFocusDocuments, moveConversationToSpace]
  );

  const handleSearchGeneral = useCallback(
    () => handleChangeSpace(GENERAL_SPACE_ID, 'General'),
    [handleChangeSpace]
  );

  const handleImportStagedFiles = useCallback(async () => {
    if (!activeConversationId || staged.length === 0 || isImportingFiles) return;
    setIsImportingFiles(true);
    try {
      const started = await VaultAPI.startBatchFileImport(staged.map((file) => file.path));
      if (!started.ok) {
        toast.error("Couldn't add these files", { message: started.error });
        return;
      }

      const jobId = started.data;
      const deadline = Date.now() + BATCH_POLL_TIMEOUT_MS;
      let documentIds: string[] = [];
      let addedCount: number | null = null;
      let failedCount = 0;
      // Poll rather than subscribe: the batch slice emits no per-job event.
      while (Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, BATCH_POLL_INTERVAL_MS));
        const status = await VaultAPI.getBatchJobStatus(jobId);
        if (!status.ok) break;
        const job = status.data;
        const terminal =
          job.status === 'completed' ||
          job.status === 'failed' ||
          job.status === 'cancelled' ||
          job.completedItems + job.failedItems >= job.totalItems;
        if (terminal) {
          documentIds = (job.items ?? [])
            .map((item) => item.documentId)
            .filter((id): id is string => Boolean(id));
          addedCount = job.completedItems;
          failedCount = job.failedItems;
          break;
        }
      }

      // Scoping only applies to a conversation that already has its own space.
      // Creating one behind the user's back would silently narrow every future
      // answer in this thread.
      if (documentIds.length > 0 && isScopedToLinkedFiles && conversationSpaceId) {
        await VaultAPI.setDocumentsSpaceMembership(documentIds, conversationSpaceId, true);
      }

      await queryClient.invalidateQueries({
        queryKey: conversationKeys.linkedDocuments(activeConversationId),
      });
      void loadConversationLinkedDocuments(activeConversationId);

      const requested = staged.length;
      clearStaged();
      // Report what the job actually did. Saying "Added 4 files" after the
      // batch failed, or after we stopped waiting, is a claim we cannot make.
      if (addedCount === null) {
        toast.info(`Still adding ${requested} file${requested !== 1 ? 's' : ''}`, {
          message: "They'll appear in this conversation when indexing finishes.",
        });
      } else if (addedCount === 0) {
        toast.error("Couldn't add these files", {
          message: `${failedCount || requested} failed to import.`,
        });
      } else {
        toast.success(`Added ${addedCount} file${addedCount !== 1 ? 's' : ''}`, {
          message:
            failedCount > 0
              ? `${failedCount} couldn't be read. The rest are indexing now.`
              : "They're indexing now.",
        });
      }
    } finally {
      setIsImportingFiles(false);
    }
  }, [
    activeConversationId,
    staged,
    isImportingFiles,
    isScopedToLinkedFiles,
    conversationSpaceId,
    queryClient,
    loadConversationLinkedDocuments,
    clearStaged,
  ]);

  const handleSwitchActiveModel = useCallback(
    async (modelId: string, modelLabel: string) => {
      const previousModelId = activeModel?.model_id ?? null;
      const previousLabel = activeModel?.model_name ?? previousModelId;
      try {
        await setActiveChatModel(modelId);
      } catch (error) {
        toast.error("Couldn't switch model", {
          message: error instanceof Error ? error.message : String(error),
        });
        return;
      }
      // The switch is app-wide, so always offer the way back — same as the
      // "Try with another model" toast on a message.
      toast.success(`Switched to ${modelLabel}`, {
        ...(previousModelId && previousModelId !== modelId
          ? {
              action: {
                label: 'Switch back',
                onClick: () => {
                  void setActiveChatModel(previousModelId).then(() => {
                    toast.success(`Back to ${previousLabel}`);
                  });
                },
              },
            }
          : {}),
      });
    },
    [activeModel, setActiveChatModel]
  );

  const turnMode: TurnMode =
    normalizeTurnMode(toolPreferences.turnMode) ?? (toolPreferences.followupMode ? 'followup' : 'auto');

  const wikipediaEnabled = WIKI_TOOL_NAMES.some((toolName) => enabledToolSet.has(toolName));
  const enabledCustomToolNames = customTools
    .map((tool) => tool.name)
    .filter((name) => enabledToolSet.has(name));

  /** What the switches are set to now: the chips and the slash rows read this. */
  const composerMode: ComposerModeState = {
    turnMode,
    knowledgeBase: Boolean(toolPreferences.knowledgeBase),
    webSearch: Boolean(toolPreferences.webSearch),
    wikipedia: wikipediaEnabled,
    deepResearch: Boolean(toolPreferences.deepResearchMode),
    canCompact: Boolean(activeConversationId) && !isCompacting && messages.length > 0,
  };

  /** A command flips exactly what its twin in the gear popover flips. */
  const runSlashCommand = (id: SlashCommandId) => {
    switch (id) {
      case 'deep':
        toggleDeepResearch();
        break;
      case 'docs':
        toggleKnowledgeBase();
        break;
      case 'web':
        toggleWebTools();
        break;
      case 'wiki':
        toggleWikiTools();
        break;
      case 'auto':
        handleTurnModeChange('auto');
        break;
      case 'followup':
        handleTurnModeChange('followup');
        break;
      case 'query':
        handleTurnModeChange('query');
        break;
      case 'compact':
        if (activeConversationId) void handleCompact(activeConversationId);
        break;
    }
  };

  const removeMode = (id: ModeChipId) => {
    switch (id) {
      case 'turn':
        handleTurnModeChange('auto');
        break;
      case 'docs':
        toggleKnowledgeBase();
        break;
      case 'web':
        toggleWebTools();
        break;
      case 'wiki':
        toggleWikiTools();
        break;
      case 'deep':
        toggleDeepResearch();
        break;
    }
  };

  // `@` may only ever offer documents of this conversation's own space — never
  // the sidebar's selection, which is a different chat's business.
  const { documents: mentionDocuments, isLoading: isLoadingMentions } = useSpaceDocuments(
    conversationSpaceId,
    mentionQuery
  );

  const focusedIds = new Set(focusDocuments.map((document) => document.documentId));

  const resolveSuggestItems = (trigger: SuggestTrigger): SuggestItem[] => {
    if (trigger.kind === 'slash') {
      return matchSlashCommands(trigger.query, composerMode).map((command) => ({
        id: command.id,
        label: `/${command.token}`,
        description: command.description,
        state: command.state(composerMode),
        icon: command.icon,
      }));
    }

    if (trigger.query !== mentionQuery) return [];
    return mentionDocuments
      .filter((document) => !focusedIds.has(document.documentId))
      .map((document) => ({
        id: document.documentId,
        label: document.fileName,
        description: document.category ?? 'In this space',
        state: null,
        icon: FileText,
      }));
  };

  const handleAcceptSuggestion = (item: SuggestItem, trigger: SuggestTrigger) => {
    // The token was the way in, not part of the question.
    const next = replaceTrigger(input, trigger, '');
    setInput(next.value);
    const textarea = textareaRef.current;
    if (textarea) {
      // The caret can only be placed once React has written the new value.
      requestAnimationFrame(() => {
        textarea.focus();
        textarea.setSelectionRange(next.caret, next.caret);
        suggest.syncCaret(textarea);
      });
    }

    if (trigger.kind === 'slash') {
      runSlashCommand(item.id as SlashCommandId);
      return;
    }

    const picked = mentionDocuments.find((document) => document.documentId === item.id);
    if (picked && !focusedIds.has(picked.documentId)) {
      applyFocusDocuments([...focusDocuments, picked]);
    }
  };

  const suggest = useComposerSuggest({
    value: input,
    textareaRef,
    resolveItems: resolveSuggestItems,
    // The list a keystroke behind is still the list: keep it open rather than
    // blinking shut between the `@` and its answer.
    isBusy: (trigger) =>
      trigger.kind === 'mention' && (isLoadingMentions || trigger.query !== mentionQuery),
    onAccept: handleAcceptSuggestion,
  });

  // The lookup follows the trigger the popup found, one render behind it.
  const activeMention = suggest.trigger?.kind === 'mention' ? suggest.trigger.query : null;
  useEffect(() => {
    setMentionQuery(activeMention);
  }, [activeMention]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // An IME sends Enter to commit a candidate. That Enter is not an accept and
    // it is certainly not a send.
    if (e.nativeEvent.isComposing) return;
    if (suggest.handleKeyDown(e)) return;
    if (e.key === 'Enter' && !e.shiftKey && !(e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit(e);
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  // Mask the input during warmup, and while there is no model to answer with.
  const isChatWarming = useModelWarmupStore(selectIsChatWarming);
  const chatWarmupPhase = useModelWarmupStore((state) => state.chat.phase);

  const llmSettings = settings?.llm;
  const resolvedModel = resolveChatModel(llmSettings, activeModel?.model_id ?? null);
  const activeModelLabel = resolvedModel && (downloadedModels.find(model => model.model_id === resolvedModel)?.model_name || resolvedModel);
  const hasChatModel = activeModelLabel !== null;
  const isChatUnavailable = isChatWarming || !hasChatModel;

  // Report the completed turn. Initial retrieval can fail and a later tool
  // search can recover while generation is still running.
  const retrievalUnavailableReason = useMemo(() => {
    if (!activeConversationId || isSending) return null;
    const conversationMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    for (let index = conversationMessages.length - 1; index >= 0; index -= 1) {
      const candidate = conversationMessages[index];
      if (candidate.role !== 'assistant') continue;
      const trace = messageRetrieval.get(candidate.id);
      if (trace && (trace.files > 0 || trace.passages > 0)) return null;
      return trace?.unavailableReason ?? null;
    }
    return null;
  }, [activeConversationId, conversations, isSending, messageRetrieval]);

  // Model state lives in the notice below the composer, not in the placeholder.
  // A chat pinned to two documents says so instead: that decides the answer.
  const focusLabel =
    focusDocuments.length > 0 ? describeFocus(focusDocuments.length, conversationSpaceName) : null;
  const placeholder =
    focusLabel ??
    (turnMode === 'followup'
      ? 'Follow up'
      : turnMode === 'query'
        ? 'Search sources and answer'
        : 'Ask anything');

  const handleCreateEmptyStateConversation = async () => {
    if (isCreatingConversation) return;
    setIsCreatingConversation(true);
    try {
      await createConversation(createDefaultConversationTitle());
    } catch (error) {
      // The controller's message names the actual fix ("Select a local model in
      // Settings or configure Ollama"); swallowing it left a dead button.
      toast.error("Couldn't start a conversation", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsCreatingConversation(false);
    }
  };

  // Deep links and failed regenerations hand the composer its text this way.
  useEffect(() => {
    if (!composerDraft) return;
    setInput(composerDraft);
    setComposerDraft(null);
    textareaRef.current?.focus();
  }, [composerDraft, setComposerDraft]);

  const lastMessage = messages.length > 0 ? messages[messages.length - 1] : null;
  const canRegenerate = Boolean(
    activeConversationId && !isSending && lastMessage?.role === 'assistant'
  );
  const paletteCommands = useMemo(
    () => [
      {
        id: 'chat.regenerate',
        label: 'Regenerate answer',
        group: 'Chat',
        icon: RefreshCw,
        enabled: canRegenerate,
        run: () => {
          if (!activeConversationId) return;
          // Re-run the turn the composer is describing, not a default one.
          void regenerateResponse(
            activeConversationId,
            resolveEffectiveToolPreferences()
          ).then((outcome) => {
            if (outcome === 'answered' || outcome === 'cancelled') return;
            toast.error("Couldn't regenerate", {
              message:
                outcome === 'busy'
                  ? 'This conversation is still answering.'
                  : 'Your question is back in the composer.',
            });
          });
        },
      },
      {
        id: 'chat.branch',
        label: 'Branch this conversation',
        group: 'Chat',
        icon: GitBranch,
        enabled: Boolean(activeConversationId) && messages.length > 0,
        run: () => {
          if (activeConversationId) {
            void forkConversation(activeConversationId).then((newId) => {
              if (newId) toast.success('Branched', { message: "You're in the new conversation." });
            });
          }
        },
      },
      {
        id: 'chat.compact',
        label: 'Compact context',
        group: 'Chat',
        icon: Scissors,
        enabled: Boolean(activeConversationId) && !isCompacting && messages.length > 0,
        description: 'Fold older messages into a summary (/compact).',
        run: () => {
          if (activeConversationId) void handleCompact(activeConversationId);
        },
      },
      {
        id: 'chat.memory',
        label: 'Show conversation memory',
        group: 'Chat',
        icon: ScrollText,
        enabled: Boolean(activeConversationId),
        description: 'What was recorded, and the quotation behind each item.',
        run: () => {
          setIsMemoryOpen(true);
        },
      },
      {
        id: 'chat.switch-model',
        label: 'Switch chat model',
        group: 'Chat',
        icon: Cpu,
        // The picker hangs off the active-model label, so the verb only works
        // when that label is on screen. No verb that does nothing.
        enabled:
          Boolean(activeModelLabel) &&
          downloadedModels.some((model) => model.model_type === 'language_model'),
        run: () => {
          modelLabelRef.current?.click();
        },
      },
      {
        id: 'chat.add-files',
        label: 'Add files to this conversation',
        group: 'Chat',
        icon: Paperclip,
        enabled: Boolean(activeConversationId),
        description: 'Or drop them onto the conversation.',
        run: () => {
          void handleChooseFiles();
        },
      },
      {
        id: 'chat.search-general',
        label: 'Move this chat to General',
        group: 'Chat',
        icon: Library,
        description: conversationSpaceName
          ? `It searches ${conversationSpaceName} now.`
          : undefined,
        enabled: isScopedToLinkedFiles,
        run: () => {
          void handleSearchGeneral();
        },
      },
    ],
    [
      activeConversationId,
      activeModelLabel,
      canRegenerate,
      downloadedModels,
      forkConversation,
      handleChooseFiles,
      handleCompact,
      handleSearchGeneral,
      conversationSpaceName,
      isCompacting,
      isScopedToLinkedFiles,
      messages.length,
      regenerateResponse,
      resolveEffectiveToolPreferences,
    ]
  );
  useRegisterPaletteCommands(paletteCommands);

  if (!activeConversationId) {
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center bg-bg">
        <div className="max-w-md px-8 text-center">
          <div className="mx-auto mb-4 flex h-11 w-11 items-center justify-center rounded-xl bg-[hsl(var(--text-primary)/0.05)] text-text-tertiary shadow-[inset_0_0_0_1px_hsl(var(--text-primary)/0.05)]">
            <MessageCircle className="h-5 w-5" strokeWidth={1.6} />
          </div>
          <p className="font-serif text-[19px] font-medium tracking-[-0.015em] text-text-primary">No conversation open.</p>
          <p className="mt-1.5 text-ui leading-relaxed text-text-muted">Pick one from the list, or start a new one and ask your library a question.</p>
          <button
            type="button"
            onClick={() => { void handleCreateEmptyStateConversation(); }}
            disabled={isCreatingConversation}
            aria-label="Create new conversation"
            className="pressable mt-5 inline-flex h-8 items-center justify-center gap-2 rounded-md bg-action px-3 text-ui font-medium text-action-fg shadow-action transition-[background-color,scale] duration-fast hover:bg-action-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            New conversation
            <kbd className="kbd bg-[hsl(var(--action-fg)/0.14)] text-[hsl(var(--action-fg)/0.8)] shadow-none">⌘N</kbd>
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={panelRef}
      data-thread={messages.length > 0 || undefined}
      className="chat-panel relative flex-1 min-w-0 flex flex-col bg-bg"
    >
      {isDragging && (
        <div className="pointer-events-none absolute inset-0 z-30 flex items-center justify-center border-2 border-dashed border-[hsl(var(--accent))] bg-bg/80 transition-opacity duration-fast">
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Drop files to add them to this conversation.
          </p>
        </div>
      )}
      <ImportFailuresNotice />
      <UtilityModelNotice />
      <ConversationMemoryPanel
        conversationId={activeConversationId}
        isOpen={isMemoryOpen}
        onClose={() => setIsMemoryOpen(false)}
        messageContentById={messageContentById}
      />
      {/* Thread scroll region */}
      <div
        ref={scrollContainerRef}
        className="flex-1 overflow-y-auto"
        onScroll={(event) => {
          const container = event.currentTarget;
          const distanceFromBottom =
            container.scrollHeight - container.scrollTop - container.clientHeight;
          shouldAutoScrollRef.current = distanceFromBottom <= 96;
        }}
      >
        <div className="chat-column mx-auto w-full">
          {messages.length === 0 ? (
            <div className="flex min-h-[50vh] flex-col items-center justify-center px-6">
              <div className="w-full max-w-[520px]">
                <ChatStarters
                  spaceId={startersSpaceId}
                  onPick={(question) => {
                    setInput(question);
                    textareaRef.current?.focus();
                  }}
                />
                <ChatEmptyStateIngestDelta />
              </div>
            </div>
          ) : (
            <motion.div
              key={activeConversationId ?? 'empty'}
              initial={prefersReducedMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              {messages.map((message, index) => {
                const key = getMessageKey(message);
                const previous = index > 0 ? messages[index - 1] : null;
                const showCompactionDivider =
                  Boolean(compactionRecord) &&
                  'id' in message &&
                  message.id === compactionRecord?.upToMessageId;
                return (
                  <Fragment key={key}>
                    <Message
                      message={message}
                      isFresh={!prefersReducedMotion && freshMessageKeys.has(key)}
                      isLastTurn={index === messages.length - 1}
                      previousMessageId={
                        previous && 'id' in previous ? previous.id : undefined
                      }
                    />
                    {showCompactionDivider && compactionRecord && (
                      <div
                        className="chat-beside-margin flex items-center gap-3 px-6 py-2"
                        role="separator"
                        aria-label="Context compacted"
                      >
                        <div className="h-px flex-1 bg-[hsl(var(--border-default))]" />
                        <span className="text-xxs uppercase tracking-wide text-[hsl(var(--text-muted))]">
                          Context compacted
                        </span>
                        <div className="h-px flex-1 bg-[hsl(var(--border-default))]" />
                      </div>
                    )}
                  </Fragment>
                );
              })}
            </motion.div>
          )}

          {/* Sticky sources-in-this-conversation footer, above the composer */}
          <div className="chat-beside-margin px-6">
            <ConversationLinkedDocumentsPanel
              conversationId={activeConversationId}
              scopedSpaceName={isScopedToLinkedFiles ? conversationSpaceName : null}
              onMoveToGeneral={() => void handleSearchGeneral()}
            />
          </div>
        </div>
      </div>

      {/* Composer pinned to the bottom of the panel. */}
      <div className="relative bg-bg before:pointer-events-none before:absolute before:inset-x-0 before:-top-8 before:h-8 before:bg-gradient-to-t before:from-[hsl(var(--bg))] before:to-transparent">
        <ChatDropStaging
          staged={staged}
          isImporting={isImportingFiles}
          onRemove={removeStaged}
          onImport={() => void handleImportStagedFiles()}
          onClear={clearStaged}
        />

        <ChatModelNotice
          hasChatModel={hasChatModel}
          warmupPhase={chatWarmupPhase}
          retrievalUnavailableReason={retrievalUnavailableReason}
        />

        <form onSubmit={handleSubmit} className="chat-column chat-beside-margin mx-auto w-full px-6 pb-5 pt-1">
          {/* One object: the page you write on, with its tools along the bottom edge. */}
          <div className="rounded-2xl bg-surface shadow-sheet transition-shadow duration-base focus-within:shadow-[var(--shadow-sheet),0_0_0_3px_hsl(var(--accent)/0.16)]">
            <FocusChips
              documents={focusDocuments}
              onRemove={(documentId) =>
                applyFocusDocuments(
                  focusDocuments.filter((document) => document.documentId !== documentId)
                )
              }
              onClear={() => applyFocusDocuments([])}
            />

            <div className="composer-caret-field">
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => {
                  setInput(e.target.value);
                  suggest.syncCaret(e.target);
                }}
                onKeyDown={handleKeyDown}
                onKeyUp={(e) => suggest.syncCaret(e.currentTarget)}
                onClick={(e) => suggest.syncCaret(e.currentTarget)}
                onFocus={() => suggest.setFocused(true)}
                onBlur={() => suggest.setFocused(false)}
                placeholder={placeholder}
                disabled={isSending}
                rows={1}
                aria-label="Message composer"
                aria-autocomplete="list"
                aria-expanded={suggest.isOpen}
                aria-controls={suggest.isOpen ? SUGGEST_LIST_ID : undefined}
                aria-activedescendant={
                  suggest.isOpen ? `${SUGGEST_LIST_ID}-${suggest.activeIndex}` : undefined
                }
                className="block w-full resize-none bg-transparent px-4 pb-1 pt-3.5 font-sans text-[15px] leading-[1.55] text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
                style={{ minHeight: '44px', maxHeight: '240px' }}
              />

              {suggest.isOpen && suggest.point && (
                <ComposerSuggest
                  items={suggest.items}
                  activeIndex={suggest.activeIndex}
                  heading={
                    suggest.trigger?.kind === 'mention'
                      ? conversationSpaceName
                        ? `Documents in ${conversationSpaceName}`
                        : 'Documents this chat can read'
                      : 'Commands'
                  }
                  emptyLabel={
                    suggest.trigger?.kind === 'mention' ? 'Looking…' : 'No command matches.'
                  }
                  point={suggest.point}
                  listId={SUGGEST_LIST_ID}
                  onSelect={suggest.accept}
                  onHover={suggest.setActiveIndex}
                />
              )}
            </div>

            {/* The tools along the bottom edge. The left group wraps when the
                modes fill it; the send button stays on the right either way. */}
            <div className="flex items-end gap-1 px-2.5 pb-2.5 pt-1">
              <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
                {/* Controls trigger (left) */}
              <Popover.Root open={isControlsOpen} onOpenChange={setIsControlsOpen}>
                <Popover.Trigger asChild>
                  <button
                    type="button"
                    aria-label="Composer controls"
                    title="Turn mode and tools"
                    className={`pressable inline-flex h-7 w-7 items-center justify-center rounded-md transition-[background-color,color,scale] duration-fast ${
                      isControlsOpen
                        ? 'bg-[hsl(var(--text-primary)/0.08)] text-[hsl(var(--text-primary))]'
                        : 'text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]'
                    }`}
                  >
                    <Settings2 className="h-[15px] w-[15px]" strokeWidth={1.6} />
                  </button>
                </Popover.Trigger>
                <Popover.Portal>
                  <Popover.Content
                    side="top"
                    align="start"
                    sideOffset={8}
                    className="surface-pop z-50 w-[320px] max-h-[480px] overflow-y-auto rounded-xl bg-surface-overlay p-4 text-[hsl(var(--text-primary))] shadow-lg outline-none"
                  >
                    <ComposerControls
                      turnMode={turnMode}
                      onTurnModeChange={handleTurnModeChange}
                      toolPreferences={toolPreferences}
                      onToggleKnowledgeBase={toggleKnowledgeBase}
                      onToggleWebTools={toggleWebTools}
                      onToggleWikiTools={toggleWikiTools}
                      onToggleDeepResearch={toggleDeepResearch}
                      customTools={customTools}
                      enabledToolSet={enabledToolSet}
                      onToggleCustomTool={toggleCustomTool}
                    />
                  </Popover.Content>
                </Popover.Portal>
              </Popover.Root>

              {/* Dropping files on the thread works, but only once you know it
                  does. The same flow the palette runs, in reach of the caret. */}
              <button
                type="button"
                onClick={() => void handleChooseFiles()}
                aria-label="Add files to this conversation"
                title="Add files to this conversation"
                className="pressable inline-flex h-7 w-7 items-center justify-center rounded-md text-[hsl(var(--text-tertiary))] transition-[background-color,color,scale] duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
              >
                <Paperclip className="h-[15px] w-[15px]" strokeWidth={1.6} />
              </button>

              {/* What a chat can search decides its answers more than the model
                  does, so it sits beside the model, where the question is typed. */}
              {showSpacePicker && (
                <SpacePickerPopover
                  activeSpaceId={conversationSpaceId}
                  heading="This chat searches"
                  onSelect={handleChangeSpace}
                >
                  <button
                    type="button"
                    title="The documents this chat searches"
                    aria-label={`${focusLabel ?? `Searching ${conversationSpaceName}`}. Change space`}
                    className="inline-flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-xs text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
                  >
                    <Library className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} aria-hidden="true" />
                    <span className="truncate">{focusLabel ?? conversationSpaceName}</span>
                    <ChevronDown className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} />
                  </button>
                </SpacePickerPopover>
              )}

              {activeModelLabel && (
                <ModelPickerPopover
                  activeModelId={activeModel?.model_id ?? null}
                  onSelect={handleSwitchActiveModel}
                  align="start"
                >
                  <button
                    ref={modelLabelRef}
                    type="button"
                    title="Active chat model"
                    className="inline-flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-xs text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]"
                  >
                    <span className="truncate">{activeModelLabel}</span>
                    <ChevronDown className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} />
                  </button>
                </ModelPickerPopover>
              )}

              <ModeChips
                turnMode={turnMode}
                knowledgeBase={composerMode.knowledgeBase}
                webSearch={composerMode.webSearch}
                wikipedia={composerMode.wikipedia}
                deepResearch={composerMode.deepResearch}
                customTools={enabledCustomToolNames}
                onRemove={removeMode}
                onRemoveTool={toggleCustomTool}
              />
              </div>

              <span className="hidden shrink-0 self-center pr-1 text-[11px] text-[hsl(var(--text-muted))] sm:block">
                {isSending ? 'Generating…' : input.trim() ? '↵ send · ⇧↵ new line' : ''}
              </span>

              {/* Send / Stop */}
              {isSending ? (
                <button
                  type="button"
                  onClick={handleCancel}
                  aria-label="Stop generating response"
                  title="Stop"
                  className="pressable inline-flex h-8 w-8 items-center justify-center rounded-full bg-[hsl(var(--text-primary))] text-[hsl(var(--bg))] transition-[scale,opacity] duration-fast hover:opacity-90"
                >
                  <Square className="h-3 w-3 fill-current" />
                </button>
              ) : (
                <button
                  type="submit"
                  disabled={!input.trim() || isChatUnavailable}
                  aria-label="Send message"
                  title="Send · Enter"
                  className="pressable inline-flex h-8 w-8 items-center justify-center rounded-full bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] shadow-action transition-[background-color,color,scale,box-shadow] duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:bg-[hsl(var(--text-primary)/0.08)] disabled:text-[hsl(var(--text-disabled))] disabled:shadow-none"
                >
                  <ArrowUp className="h-4 w-4" strokeWidth={2.2} />
                </button>
              )}
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}
