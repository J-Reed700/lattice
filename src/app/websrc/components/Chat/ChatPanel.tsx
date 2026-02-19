import { useEffect, useMemo, useRef, useState } from 'react';

import {
  AlertCircle,
  BookOpen,
  ChevronDown,
  ChevronUp,
  Database,
  Globe,
  MessageSquarePlus,
  Send,
  SlidersHorizontal,
  Sparkles,
  Square,
  Wrench,
  X,
} from 'lucide-react';

import { ConversationLinkedDocumentsPanel } from './ConversationLinkedDocumentsPanel';
import { MessageBubble } from './MessageBubble';
import { VaultAPI } from '../../lib/api';
import { getConversationMessages, useConversationsStore } from '../../stores/conversationsStore';

import type { CustomToolSettings, ToolPreferences } from '../../types';

const WEB_TOOL_NAMES = ['web_search', 'fetch_url_content'] as const;
const WIKI_TOOL_NAMES = ['wiki_search', 'wiki_summary'] as const;
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

const formatToolLabel = (name: string): string =>
  name
    .split('_')
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(' ');

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

const loadInitialToolPreferences = (): ToolPreferences => {
  try {
    const stored = localStorage.getItem('toolPreferences');
    if (!stored) {
      return defaultToolPreferences();
    }

    const parsed = JSON.parse(stored) as Record<string, unknown>;
    if (typeof parsed.knowledgeBase !== 'boolean' || typeof parsed.webSearch !== 'boolean') {
      return defaultToolPreferences();
    }
    const deepResearchMode =
      typeof parsed.deepResearchMode === 'boolean'
        ? parsed.deepResearchMode
        : (typeof parsed.deep_research_mode === 'boolean' ? parsed.deep_research_mode : false);
    const turnMode = resolveTurnMode(parsed);
    const followupMode = turnMode === 'followup';

    const enabledTools = normalizeEnabledTools(parsed.enabledTools);
    if (enabledTools) {
      return {
        knowledgeBase: parsed.knowledgeBase,
        webSearch: parsed.webSearch,
        deepResearchMode,
        followupMode,
        turnMode,
        enabledTools,
      };
    }

    return {
      knowledgeBase: parsed.knowledgeBase,
      webSearch: parsed.webSearch,
      deepResearchMode,
      followupMode,
      turnMode,
      enabledTools: parsed.webSearch ? [...WEB_TOOL_NAMES] : [],
    };
  } catch {
    return defaultToolPreferences();
  }
};

export function ChatPanel() {
  const {
    activeConversationId,
    conversations,
    spaces,
    isSending,
    error,
    sendMessage,
    cancelGeneration,
    clearError,
    optimisticMessages,
  } = useConversationsStore();

  const [input, setInput] = useState('');
  const [toolPreferences, setToolPreferences] = useState<ToolPreferences>(loadInitialToolPreferences);
  const [showComposerControls, setShowComposerControls] = useState(false);
  const [customTools, setCustomTools] = useState<CustomToolSettings[]>([]);
  const toolPreferencesRef = useRef<ToolPreferences>(toolPreferences);
  const lastAppliedToolPreferenceConversationRef = useRef<string | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const previousConversationIdRef = useRef<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const enabledToolSet = useMemo(
    () => new Set(toolPreferences.enabledTools ?? []),
    [toolPreferences.enabledTools]
  );
  const wikiEnabled = WIKI_TOOL_NAMES.some((toolName) => enabledToolSet.has(toolName));

  // Use selector to get combined real + optimistic messages
  const messages = getConversationMessages(activeConversationId);

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

    container.scrollTo({
      top: container.scrollHeight,
      behavior: isConversationChange ? 'auto' : 'smooth',
    });
  }, [messages, optimisticMessages, activeConversationId]);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${textareaRef.current.scrollHeight}px`;
    }
  }, [input]);

  useEffect(() => {
    toolPreferencesRef.current = toolPreferences;
    try {
      localStorage.setItem('toolPreferences', JSON.stringify(toolPreferences));
    } catch {
      // Ignore storage errors (e.g., storage disabled)
    }
  }, [toolPreferences]);

  useEffect(() => {
    if (!activeConversationId) {
      lastAppliedToolPreferenceConversationRef.current = null;
      return;
    }
    if (lastAppliedToolPreferenceConversationRef.current === activeConversationId) {
      return;
    }

    const activeConversation = conversations.find(
      (conversation) => conversation.id === activeConversationId
    );
    const spaceId = activeConversation?.spaceId;
    if (!spaceId) {
      lastAppliedToolPreferenceConversationRef.current = activeConversationId;
      return;
    }

    const space = spaces.find((item) => item.id === spaceId);
    if (!space?.toolPreferencesJson) {
      lastAppliedToolPreferenceConversationRef.current = activeConversationId;
      return;
    }

    try {
      const parsed = JSON.parse(space.toolPreferencesJson) as Record<string, unknown>;
      const knowledgeBase =
        typeof parsed.knowledgeBase === 'boolean'
          ? parsed.knowledgeBase
          : (typeof parsed.knowledge_base === 'boolean' ? parsed.knowledge_base : undefined);
      const webSearch =
        typeof parsed.webSearch === 'boolean'
          ? parsed.webSearch
          : (typeof parsed.web_search === 'boolean' ? parsed.web_search : undefined);
      const turnMode = resolveTurnMode(parsed);
      const followupMode = turnMode === 'followup';
      const deepResearchMode =
        typeof parsed.deepResearchMode === 'boolean'
          ? parsed.deepResearchMode
          : (typeof parsed.deep_research_mode === 'boolean' ? parsed.deep_research_mode : false);

      if (typeof knowledgeBase === 'boolean' && typeof webSearch === 'boolean') {
        const explicitEnabledTools =
          normalizeEnabledTools(parsed.enabledTools) ??
          normalizeEnabledTools(parsed.enabled_tools);
        const enabledTools =
          explicitEnabledTools ??
          setToolNames(toolPreferencesRef.current.enabledTools, WEB_TOOL_NAMES, webSearch);
        const next = {
          knowledgeBase,
          webSearch,
          deepResearchMode,
          followupMode,
          turnMode,
          enabledTools,
        };
        setToolPreferences(next);
        toolPreferencesRef.current = next;
      }
    } catch {
      // Ignore malformed per-space tool preference JSON
    }

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
    updateToolPreferences((prev) => ({
      ...prev,
      deepResearchMode: !(prev.deepResearchMode ?? false),
    }));
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

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isSending || !activeConversationId) return;

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

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const toolButtonClass = (active: boolean) =>
    `inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs transition-all ${
      active
        ? 'bg-blue-500/20 text-blue-200 border-blue-500/40'
        : 'bg-white/5 text-white/50 border-white/10 hover:bg-white/10'
    }`;
  const turnModeButtonClass = (active: boolean) =>
    `inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs transition-all ${
      active
        ? 'bg-emerald-500/20 text-emerald-200 border-emerald-500/40'
        : 'bg-white/5 text-white/60 border-white/10 hover:bg-white/10'
    }`;
  const turnMode =
    normalizeTurnMode(toolPreferences.turnMode) ?? (toolPreferences.followupMode ? 'followup' : 'auto');
  const customEnabledCount = customTools.filter((tool) => enabledToolSet.has(tool.name)).length;
  const activeComposerFlags = [
    turnMode !== 'auto' ? `Mode: ${turnMode}` : null,
    toolPreferences.knowledgeBase ? 'KB' : null,
    toolPreferences.webSearch ? 'Web' : null,
    wikiEnabled ? 'Wiki' : null,
    toolPreferences.deepResearchMode ? 'Deep' : null,
    customEnabledCount > 0 ? `${customEnabledCount} Custom` : null,
  ].filter((value): value is string => Boolean(value));
  const visibleComposerFlags = activeComposerFlags.slice(0, 3);
  const hiddenComposerFlagCount = Math.max(0, activeComposerFlags.length - visibleComposerFlags.length);

  if (!activeConversationId) {
    return (
      <div className="flex-1 flex items-center justify-center bg-gradient-to-b from-[#0d1117] to-[#0a0e14]">
        <div className="text-center px-8">
          <div className="w-20 h-20 mx-auto mb-6 rounded-2xl bg-gradient-to-br from-blue-500/20 to-purple-500/20 border border-blue-500/30 flex items-center justify-center backdrop-blur-xl">
            <MessageSquarePlus className="w-10 h-10 text-blue-400" />
          </div>
          <h2 className="text-2xl font-bold text-white/90 mb-3">
            No Conversation Selected
          </h2>
          <p className="text-white/60 max-w-md">
            Select an existing conversation from the sidebar or create a new one to start chatting.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col bg-gradient-to-b from-[#0d1117] to-[#0a0e14]">
      {error && (
        <div className="bg-red-500/10 border-b border-red-500/20 px-6 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <AlertCircle className="w-5 h-5 text-red-400" />
            <p className="text-sm text-red-300">{error}</p>
          </div>
          <button
            onClick={clearError}
            aria-label="Dismiss error"
            className="p-1 rounded-lg hover:bg-red-500/20 text-red-400 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      <ConversationLinkedDocumentsPanel conversationId={activeConversationId} />

      <div ref={scrollContainerRef} className="flex-1 overflow-y-auto">
        {messages.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center px-8">
              <div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-gradient-to-br from-green-500/20 to-teal-500/20 border border-green-500/30 flex items-center justify-center backdrop-blur-xl">
                <MessageSquarePlus className="w-8 h-8 text-green-400" />
              </div>
              <h3 className="text-lg font-semibold text-white/90 mb-2">
                Start a Conversation
              </h3>
              <p className="text-white/60 text-sm max-w-sm">
                Ask me anything! I can help you search your documents, answer questions, and more.
              </p>
            </div>
          </div>
        ) : (
          <div className="divide-y divide-white/5">
            {messages.map((message) => (
              <MessageBubble
                key={'tempId' in message ? message.tempId : message.id}
                message={message}
              />
            ))}
          </div>
        )}
      </div>

      <div className="border-t border-white/[0.06] bg-gradient-to-t from-black/30 to-transparent backdrop-blur-2xl">
        <form onSubmit={handleSubmit} className="p-4">
          <div className="relative">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={
                turnMode === 'followup'
                  ? 'Ask a follow-up about this conversation...'
                  : (turnMode === 'query'
                    ? 'Ask a query (retrieval + web enrichment run automatically)...'
                    : 'Ask a question or type a message...')
              }
              disabled={isSending}
              rows={1}
              aria-label="Message input"
              className="w-full px-4 py-3 pr-12 bg-white/[0.04] border border-white/[0.08] rounded-2xl text-white/90 placeholder-white/40 resize-none focus:outline-none focus:ring-2 focus:ring-blue-500/30 focus:border-blue-500/30 focus:bg-white/[0.06] transition-all duration-200 disabled:opacity-50 disabled:cursor-not-allowed max-h-40"
              style={{ minHeight: '48px' }}
            />
            {isSending ? (
              <button
                type="button"
                onClick={handleCancel}
                aria-label="Stop generation"
                className="absolute right-2 bottom-2 p-2 rounded-xl bg-gradient-to-r from-rose-500/20 to-orange-500/20 hover:from-rose-500/30 hover:to-orange-500/30 border border-rose-500/30 text-rose-300 hover:text-rose-200 shadow-lg shadow-rose-500/10 transition-all duration-200 backdrop-blur-sm"
                title="Stop generation"
              >
                <Square className="w-4 h-4" />
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim() || isSending}
                aria-label="Send message"
                className="absolute right-2 bottom-2 p-2 rounded-xl bg-gradient-to-r from-blue-500/20 to-indigo-500/20 hover:from-blue-500/30 hover:to-indigo-500/30 border border-blue-500/30 text-blue-400 hover:text-blue-300 shadow-lg shadow-blue-500/10 transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed backdrop-blur-sm"
                title="Send message (Enter)"
              >
                <Send className="w-4 h-4" />
              </button>
            )}
          </div>
          <div className="mt-2">
            <div className="flex items-center justify-between gap-2 text-[11px] text-white/40">
              <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                {activeComposerFlags.length === 0 ? (
                  <span className="truncate text-white/35">Auto mode · minimal controls</span>
                ) : (
                  <>
                    {visibleComposerFlags.map((flag) => (
                    <span
                      key={flag}
                      className="rounded-full border border-white/15 bg-white/[0.04] px-2 py-0.5 text-white/65"
                    >
                      {flag}
                    </span>
                    ))}
                    {hiddenComposerFlagCount > 0 && (
                      <span className="rounded-full border border-white/15 bg-white/[0.04] px-2 py-0.5 text-white/55">
                        +{hiddenComposerFlagCount}
                      </span>
                    )}
                  </>
                )}
              </div>
              <button
                type="button"
                onClick={() => setShowComposerControls((prev) => !prev)}
                className="inline-flex items-center gap-1 rounded-md border border-white/15 bg-white/[0.04] px-2 py-1 text-white/70 transition-colors hover:border-white/30 hover:text-white"
              >
                <SlidersHorizontal className="h-3 w-3" />
                Controls
                {showComposerControls ? (
                  <ChevronUp className="h-3 w-3" />
                ) : (
                  <ChevronDown className="h-3 w-3" />
                )}
              </button>
            </div>

            {showComposerControls && (
              <div className="mt-2 space-y-2 rounded-xl border border-white/10 bg-white/[0.03] p-2.5 text-xs text-white/40">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[10px] uppercase tracking-wider text-white/35">Turn</span>
                  <button
                    type="button"
                    className={turnModeButtonClass(turnMode === 'auto')}
                    aria-pressed={turnMode === 'auto'}
                    onClick={() =>
                      updateToolPreferences((prev) => ({
                        ...prev,
                        followupMode: false,
                        turnMode: 'auto',
                      }))
                    }
                    title="Auto: let the assistant infer whether this is a new topic or follow-up"
                  >
                    Auto
                  </button>
                  <button
                    type="button"
                    className={turnModeButtonClass(turnMode === 'followup')}
                    aria-pressed={turnMode === 'followup'}
                    onClick={() =>
                      updateToolPreferences((prev) => ({
                        ...prev,
                        followupMode: true,
                        turnMode: 'followup',
                      }))
                    }
                    title="Follow-up: treat this turn as context-dependent and prefer KB-first when KB and web are both enabled"
                  >
                    <MessageSquarePlus className="h-3 w-3" />
                    Follow-up
                  </button>
                  <button
                    type="button"
                    className={turnModeButtonClass(turnMode === 'query')}
                    aria-pressed={turnMode === 'query'}
                    onClick={() =>
                      updateToolPreferences((prev) => ({
                        ...prev,
                        followupMode: false,
                        turnMode: 'query',
                      }))
                    }
                    title="Query: force full retrieval mode for this turn (knowledge base + enabled tools)"
                  >
                    <span className="inline-flex items-center gap-0.5" aria-hidden="true">
                      <Database className="h-3 w-3" />
                      <Wrench className="h-3 w-3" />
                    </span>
                    Query
                  </button>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[10px] uppercase tracking-wider text-white/35">Tools</span>
                  <button
                    type="button"
                    className={toolButtonClass(toolPreferences.knowledgeBase)}
                    aria-pressed={toolPreferences.knowledgeBase}
                    onClick={toggleKnowledgeBase}
                    title="Force knowledge base retrieval for this turn"
                  >
                    <Database className="h-3 w-3" />
                    KB
                  </button>
                  <button
                    type="button"
                    className={toolButtonClass(toolPreferences.webSearch)}
                    aria-pressed={toolPreferences.webSearch}
                    onClick={toggleWebTools}
                    title="Enable and force web retrieval for this turn"
                  >
                    <Globe className="h-3 w-3" />
                    Web
                  </button>
                  <button
                    type="button"
                    className={toolButtonClass(wikiEnabled)}
                    aria-pressed={wikiEnabled}
                    onClick={toggleWikiTools}
                    title="Enable Wikipedia search + summary tools"
                  >
                    <BookOpen className="h-3 w-3" />
                    Wiki
                  </button>
                  <button
                    type="button"
                    className={toolButtonClass(Boolean(toolPreferences.deepResearchMode))}
                    aria-pressed={Boolean(toolPreferences.deepResearchMode)}
                    onClick={toggleDeepResearch}
                    title="Run recursive deep research (multi-provider, multi-step web expansion)"
                  >
                    <Sparkles className="h-3 w-3" />
                    Deep
                  </button>
                  {customTools.map((tool) => {
                    const enabled = enabledToolSet.has(tool.name);
                    return (
                      <button
                        key={tool.name}
                        type="button"
                        className={toolButtonClass(enabled)}
                        aria-pressed={enabled}
                        onClick={() => toggleCustomTool(tool.name)}
                        title={tool.description || tool.name}
                      >
                        <Wrench className="h-3 w-3" />
                        {formatToolLabel(tool.name)}
                      </button>
                    );
                  })}
                  {isSending && <span className="text-blue-400">Sending...</span>}
                </div>
              </div>
            )}

            <div className="mt-2 text-[11px] text-white/35">Press Enter to send, Shift+Enter for new line</div>
          </div>
        </form>
      </div>
    </div>
  );
}
