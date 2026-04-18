import { useEffect, useMemo, useRef, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { motion, useReducedMotion } from 'framer-motion';
import { MessageSquare, Plus, Send, Settings2, Square } from 'lucide-react';

import { ComposerControls, WEB_TOOL_NAMES, WIKI_TOOL_NAMES, DEEP_RESEARCH_WARNING_MESSAGE } from './ComposerControls';
import { ConversationLinkedDocumentsPanel } from './ConversationLinkedDocumentsPanel';
import { Message } from './Message';
import { VaultAPI } from '../../lib/api';
import { getConversationMessages, useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';
import { createDefaultConversationTitle } from '../../utils/conversationTitles';

import type { CustomToolSettings, ToolPreferences } from '../../types';

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
    sendMessage,
    cancelGeneration,
    createConversation,
    optimisticMessages,
  } = useConversationsStore();

  const [input, setInput] = useState('');
  const [toolPreferences, setToolPreferences] = useState<ToolPreferences>(loadInitialToolPreferences);
  const [customTools, setCustomTools] = useState<CustomToolSettings[]>([]);
  const [isControlsOpen, setIsControlsOpen] = useState(false);
  const [isCreatingConversation, setIsCreatingConversation] = useState(false);
  const toolPreferencesRef = useRef<ToolPreferences>(toolPreferences);
  const lastAppliedToolPreferenceConversationRef = useRef<string | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const previousConversationIdRef = useRef<string | null>(null);
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

  const messages = getConversationMessages(activeConversationId);

  const getMessageKey = (message: typeof messages[number]): string =>
    'tempId' in message ? message.tempId : message.id;

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
  }, [activeConversationId, messages]);

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
      // Ignore storage errors
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
    } catch {
      // Error surfaces via conversationsStore
    } finally {
      setIsCreatingConversation(false);
    }
  };

  if (!activeConversationId) {
    return (
      <div className="flex-1 min-w-0 flex items-center justify-center bg-bg">
        <div className="text-center px-8 max-w-md">
          <MessageSquare className="mx-auto mb-4 h-8 w-8 text-[hsl(var(--text-muted))]" aria-hidden="true" />
          <h2 className="text-lg font-semibold font-serif text-[hsl(var(--text-primary))]">
            No conversation open.
          </h2>
          <button
            type="button"
            onClick={() => { void handleCreateEmptyStateConversation(); }}
            disabled={isCreatingConversation}
            aria-label="Create new conversation"
            className="mt-4 inline-flex items-center justify-center gap-2 rounded-md bg-[hsl(var(--accent))] px-4 py-2 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>New conversation</span>
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 min-w-0 flex flex-col bg-bg">
      {/* Thread scroll region */}
      <div ref={scrollContainerRef} className="flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-[clamp(680px,72vw,900px)]">
          {messages.length === 0 ? (
            <div className="flex items-center justify-center min-h-[60vh] px-6">
              <div className="text-center max-w-md">
                <MessageSquare className="mx-auto mb-4 h-8 w-8 text-[hsl(var(--text-muted))]" aria-hidden="true" />
                <h3 className="text-lg font-semibold font-serif text-[hsl(var(--text-primary))]">
                  No messages yet.
                </h3>
              </div>
            </div>
          ) : (
            <motion.div
              key={activeConversationId ?? 'empty'}
              initial={prefersReducedMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              {messages.map((message) => {
                const key = getMessageKey(message);
                return (
                  <Message
                    key={key}
                    message={message}
                    isFresh={!prefersReducedMotion && freshMessageKeys.has(key)}
                  />
                );
              })}
            </motion.div>
          )}

          {/* Sticky sources-in-this-conversation footer, above the composer */}
          <div className="px-6">
            <ConversationLinkedDocumentsPanel conversationId={activeConversationId} />
          </div>
        </div>
      </div>

      {/* Composer — pinned bottom (CHAT-REDESIGN-SPEC §4) */}
      <div className="border-t border-subtle bg-bg">
        <form onSubmit={handleSubmit} className="mx-auto w-full max-w-[clamp(680px,72vw,900px)] px-6 py-4">
          <div className="relative rounded-md border border-default bg-surface focus-within:ring-2 focus-within:ring-[hsl(var(--ring))]">
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
                disabled={!input.trim()}
                aria-label="Send message"
                title="Send · Enter"
                className="absolute right-3 bottom-3 inline-flex h-8 w-8 items-center justify-center rounded-sm bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] transition-[background-color,color,transform] duration-fast active:scale-[0.97] motion-reduce:active:scale-100 motion-reduce:transition-none hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:bg-[hsl(var(--border-default))] disabled:text-[hsl(var(--text-muted))] disabled:active:scale-100"
              >
                <Send className="h-4 w-4" />
              </button>
            )}
          </div>
          <div className="mt-2 text-right text-xs text-[hsl(var(--text-muted))]">
            <kbd className="font-mono">Enter</kbd> to send · <kbd className="font-mono">Shift</kbd> + <kbd className="font-mono">Enter</kbd> for new line
          </div>
        </form>
      </div>
    </div>
  );
}
