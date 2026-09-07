import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { useQueryClient } from '@tanstack/react-query';
import { motion, useReducedMotion } from 'framer-motion';
import { Cpu, Globe, GitBranch, Paperclip, RefreshCw, Send, Settings2, Square } from 'lucide-react';

import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';

import { ChatDropStaging } from './ChatDropStaging';
import { ChatEmptyStateIngestDelta } from './ChatEmptyStateIngestDelta';
import { ChatModelNotice } from './ChatModelNotice';
import { ChatStarters } from './ChatStarters';
import { ComposerControls, WEB_TOOL_NAMES, WIKI_TOOL_NAMES, DEEP_RESEARCH_WARNING_MESSAGE } from './ComposerControls';
import { ConversationLinkedDocumentsPanel } from './ConversationLinkedDocumentsPanel';
import { Message } from './Message';
import { ModelPickerPopover } from './ModelPickerPopover';
import { useChatFileDrop } from './useChatFileDrop';
import { useSettingsQuery } from '../../hooks/queries/useSettingsQuery';
import { conversationKeys } from '../../hooks/useConversationsController';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { selectIsChatWarming, useModelWarmupStore } from '../../stores/modelWarmupStore';
import { toast } from '../../stores/toastStore';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';

import type { CustomToolSettings, ToolPreferences } from '../../types';

const GENERAL_SPACE_ID = 'space_general';

/** Poll a batch job until it stops moving, or five minutes elapse. */
const BATCH_POLL_INTERVAL_MS = 1000;
const BATCH_POLL_TIMEOUT_MS = 5 * 60 * 1000;

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
    inFlightGenerations,
    sendMessage,
    cancelGeneration,
    createConversation,
    optimisticMessages,
    liveRetrieval,
    messageRetrieval,
    composerDraft,
    setComposerDraft,
    regenerateResponse,
    forkConversation,
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

  const updateToolPreferences = (updater: (prev: ToolPreferences) => ToolPreferences) => {
    setToolPreferences((prev) => {
      const next = updater(prev);
      toolPreferencesRef.current = next;
      return next;
    });
  };

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
    const currentToolPreferences = toolPreferencesRef.current;
    const turnMode =
      normalizeTurnMode(currentToolPreferences.turnMode) ??
      (currentToolPreferences.followupMode ? 'followup' : 'auto');
    const queryModeEnabledTools =
      turnMode === 'query'
        ? [
            ...new Set([
              ...(currentToolPreferences.enabledTools ?? []),
              ...WEB_TOOL_NAMES,
              ...customTools.map((tool) => tool.name),
            ]),
          ]
        : currentToolPreferences.enabledTools;

    const effectiveToolPreferences: ToolPreferences = {
      ...currentToolPreferences,
      turnMode,
      followupMode: turnMode === 'followup',
      knowledgeBase: turnMode === 'query' ? true : currentToolPreferences.knowledgeBase,
      webSearch: turnMode === 'query' ? true : currentToolPreferences.webSearch,
      enabledTools: queryModeEnabledTools,
    };

    await sendMessage(message, activeConversationId, effectiveToolPreferences);
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

  const handleSearchWholeVault = useCallback(async () => {
    if (!activeConversationId) return;
    await moveConversationToSpace(activeConversationId, GENERAL_SPACE_ID);
    toast.success('Searching your whole vault.');
  }, [activeConversationId, moveConversationToSpace]);

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

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !(e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit(e);
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const turnMode: TurnMode =
    normalizeTurnMode(toolPreferences.turnMode) ?? (toolPreferences.followupMode ? 'followup' : 'auto');

  // Mask the input during warmup, and while there is no model to answer with.
  const isChatWarming = useModelWarmupStore(selectIsChatWarming);
  const chatWarmupPhase = useModelWarmupStore((state) => state.chat.phase);

  // An Ollama-only install has no `is_active_for_chat` row, but the controller
  // will happily build a conversation from the configured endpoint. Reading
  // only `activeModel` left those users with a dead send button forever.
  const llmSettings = settings?.llm;
  const hasOllamaChat =
    Boolean(llmSettings?.ollamaUrl && llmSettings?.model) &&
    (llmSettings?.provider === 'ollama' || llmSettings?.provider === 'auto');
  const hasChatModel = activeModel !== null || hasOllamaChat;
  const isChatUnavailable = isChatWarming || !hasChatModel;

  const activeModelLabel = activeModel
    ? activeModel.model_name || activeModel.model_id
    : hasOllamaChat
      ? (llmSettings?.model ?? null)
      : null;

  // The last thing the backend told us about why it could not read the vault.
  // Never inferred here.
  const retrievalUnavailableReason = useMemo(() => {
    if (!activeConversationId) return null;
    const live = liveRetrieval.get(activeConversationId);
    if (live?.unavailableReason) return live.unavailableReason;
    const conversationMessages =
      conversations.find((conversation) => conversation.id === activeConversationId)?.messages ?? [];
    for (let index = conversationMessages.length - 1; index >= 0; index -= 1) {
      const candidate = conversationMessages[index];
      if (candidate.role !== 'assistant') continue;
      return messageRetrieval.get(candidate.id)?.unavailableReason ?? null;
    }
    return null;
  }, [activeConversationId, conversations, liveRetrieval, messageRetrieval]);

  // Model state lives in the notice below the composer, not in the placeholder.
  const placeholder =
    turnMode === 'followup'
      ? 'Follow up'
      : turnMode === 'query'
        ? 'Search sources and answer'
        : 'Ask anything';

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
          void regenerateResponse(activeConversationId).then((outcome) => {
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
        id: 'chat.search-vault',
        label: 'Search the whole vault',
        group: 'Chat',
        icon: Globe,
        enabled: isScopedToLinkedFiles,
        run: () => {
          void handleSearchWholeVault();
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
      handleSearchWholeVault,
      isScopedToLinkedFiles,
      messages.length,
      regenerateResponse,
    ]
  );
  useRegisterPaletteCommands(paletteCommands);

  if (!activeConversationId) {
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center bg-bg">
        <div className="max-w-md px-8 text-center">
          <p className="text-sm text-text-secondary">No conversation open.</p>
          <button
            type="button"
            onClick={() => { void handleCreateEmptyStateConversation(); }}
            disabled={isCreatingConversation}
            aria-label="Create new conversation"
            className="mt-4 inline-flex items-center justify-center rounded-sm border border-border-default px-3 py-1.5 text-sm text-text-primary transition-colors duration-fast hover:bg-surface disabled:cursor-not-allowed disabled:opacity-50"
          >
            New conversation
          </button>
        </div>
      </div>
    );
  }

  return (
    <div ref={panelRef} className="relative flex-1 min-w-0 flex flex-col bg-bg">
      {isDragging && (
        <div className="pointer-events-none absolute inset-0 z-30 flex items-center justify-center border-2 border-dashed border-[hsl(var(--accent))] bg-bg/80 transition-opacity duration-fast">
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Drop files to add them to this conversation.
          </p>
        </div>
      )}
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
        <div className="mx-auto w-full max-w-[clamp(680px,72vw,900px)]">
          {messages.length === 0 ? (
            <div className="flex min-h-[50vh] flex-col items-center justify-center px-6">
              <div className="w-full max-w-[520px]">
                <ChatStarters
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
                return (
                  <Message
                    key={key}
                    message={message}
                    isFresh={!prefersReducedMotion && freshMessageKeys.has(key)}
                    isLastTurn={index === messages.length - 1}
                    previousMessageId={
                      previous && 'id' in previous ? previous.id : undefined
                    }
                  />
                );
              })}
            </motion.div>
          )}

          {/* Sticky sources-in-this-conversation footer, above the composer */}
          <div className="px-6">
            <ConversationLinkedDocumentsPanel
              conversationId={activeConversationId}
              isScopedToLinkedFiles={isScopedToLinkedFiles}
              onSearchWholeVault={() => void handleSearchWholeVault()}
            />
          </div>
        </div>
      </div>

      {/* Composer — pinned bottom (CHAT-REDESIGN-SPEC §4) */}
      <div className="border-t border-subtle bg-bg">
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

        {activeModelLabel && (
          <div className="mx-auto w-full max-w-[clamp(680px,72vw,900px)] px-6 pb-1 pt-2">
            <ModelPickerPopover
              activeModelId={activeModel?.model_id ?? null}
              onSelect={handleSwitchActiveModel}
              align="start"
            >
              <button
                ref={modelLabelRef}
                type="button"
                title="Active chat model"
                className="text-xxs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
              >
                {activeModelLabel}
              </button>
            </ModelPickerPopover>
          </div>
        )}

        <form onSubmit={handleSubmit} className="mx-auto w-full max-w-[clamp(680px,72vw,900px)] px-6 py-4">
          <div className="relative rounded-md border border-border-default bg-surface focus-within:ring-2 focus-within:ring-[hsl(var(--ring))]">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={placeholder}
              disabled={isSending}
              rows={1}
              aria-label="Message composer"
              className="w-full resize-none bg-transparent px-4 py-3 pl-11 pr-11 font-sans text-base text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
              style={{ minHeight: '48px', maxHeight: '240px' }}
            />

            {/* Controls trigger (left) */}
            <Popover.Root open={isControlsOpen} onOpenChange={setIsControlsOpen}>
              <Popover.Trigger asChild>
                <button
                  type="button"
                  aria-label="Composer controls"
                  title="Turn mode and tools"
                  className={`absolute left-3 bottom-3 inline-flex h-6 w-6 items-center justify-center rounded-sm transition-colors duration-fast active:scale-[0.97] motion-reduce:active:scale-100 motion-reduce:transition-none ${
                    isControlsOpen
                      ? 'text-[hsl(var(--text-secondary))]'
                      : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]'
                  }`}
                >
                  <Settings2 className="h-4 w-4" />
                </button>
              </Popover.Trigger>
              <Popover.Portal>
                <Popover.Content
                  side="top"
                  align="start"
                  sideOffset={8}
                  className="z-50 w-[320px] max-h-[480px] overflow-y-auto rounded-md border border-subtle bg-surface-raised p-4 text-[hsl(var(--text-primary))] shadow-md outline-none data-[state=open]:animate-in data-[state=open]:duration-base data-[state=open]:ease-out data-[state=closed]:animate-out data-[state=closed]:duration-fast data-[state=closed]:ease-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95"
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

            {/* Send / Stop (right) */}
            {isSending ? (
              <button
                type="button"
                onClick={handleCancel}
                aria-label="Stop generating response"
                title="Stop"
                className="absolute right-3 bottom-3 inline-flex h-8 w-8 items-center justify-center rounded-sm bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))] transition-[background-color,color,transform] duration-fast active:scale-[0.97] motion-reduce:active:scale-100 motion-reduce:transition-none hover:brightness-110"
              >
                <Square className="h-4 w-4" />
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim() || isChatUnavailable}
                aria-label="Send message"
                title="Send · Enter"
                className="absolute right-3 bottom-3 inline-flex h-8 w-8 items-center justify-center rounded-sm bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] transition-[background-color,color,transform] duration-fast active:scale-[0.97] motion-reduce:active:scale-100 motion-reduce:transition-none hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:bg-[hsl(var(--border-default))] disabled:text-[hsl(var(--text-muted))] disabled:active:scale-100"
              >
                <Send className="h-4 w-4" />
              </button>
            )}
          </div>
        </form>
      </div>
    </div>
  );
}
