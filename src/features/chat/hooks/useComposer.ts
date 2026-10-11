import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';

import { FileText } from 'lucide-react';

import type { SuggestItem } from '@/features/chat/components/composer/ComposerSuggest';
import { describeFocus } from '@/features/chat/components/composer/FocusChips';
import type { ModeChipId } from '@/features/chat/components/composer/ModeChips';
import { matchSlashCommands, parseSlashSubmission } from '@/features/chat/components/composer/slashCommands';
import type { ComposerModeState, SlashCommandId } from '@/features/chat/components/composer/slashCommands';
import { replaceTrigger } from '@/features/chat/components/composer/suggestTrigger';
import type { SuggestTrigger } from '@/features/chat/components/composer/suggestTrigger';
import { WEB_TOOL_NAMES, WIKI_TOOL_NAMES } from '@/features/chat/components/composer/toolNames';
import { useComposerSuggest } from '@/features/chat/components/composer/useComposerSuggest';
import { useRefocusAfterTurn } from '@/features/chat/components/composer/useRefocusAfterTurn';
import { DEEP_RESEARCH_WARNING_MESSAGE, type TurnMode } from '@/features/chat/components/ComposerControls';
import type { AttachmentImport } from '@/features/chat/hooks/useAttachmentImport';
import { useSpaceDocuments } from '@/features/chat/hooks/useSpaceDocuments';
import { useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { CustomToolSettings, SpaceDocument, ToolPreferences } from '@/types';

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

interface ComposerOptions {
  conversationId: string | null;
  /** The space the conversation searches, and its name, for `@` and the placeholder. */
  conversationSpaceId: string | null;
  conversationSpaceName: string | null;
  isSending: boolean;
  isCompacting: boolean;
  isChatUnavailable: boolean;
  /** Whether `/compact` has anything to fold. */
  canCompact: boolean;
  onCompact: (_conversationId: string) => void;
  attachments: Pick<AttachmentImport, 'staged' | 'importStaged'>;
}

/**
 * The composer's state: the draft, the switches a turn goes out with, the
 * documents it is pinned to and the `/` and `@` menu. Everything a turn sends
 * is decided here; the panel around it only lays it out.
 */
export function useComposer({
  conversationId,
  conversationSpaceId,
  conversationSpaceName,
  isSending,
  isCompacting,
  isChatUnavailable,
  canCompact,
  onCompact,
  attachments,
}: ComposerOptions) {
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const composerDraft = useConversationsStore((state) => state.composerDraft);
  const setComposerDraft = useConversationsStore((state) => state.setComposerDraft);
  const sendMessage = useConversationsStore((state) => state.sendMessage);
  const settings = useSettingsQuery().data;
  const customTools = useMemo<CustomToolSettings[]>(
    () => (settings?.llm.customTools ?? [])
      .filter((tool) => tool.enabled && tool.name.trim().length > 0)
      .sort((a, b) => a.name.localeCompare(b.name)),
    [settings],
  );

  const [input, setInput] = useState('');
  const [toolPreferences, setToolPreferences] = useState<ToolPreferences>(defaultToolPreferences);
  const [isControlsOpen, setIsControlsOpen] = useState(false);
  const [focusDocuments, setFocusDocuments] = useState<SpaceDocument[]>([]);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const toolPreferencesRef = useRef<ToolPreferences>(toolPreferences);
  const lastAppliedToolPreferenceConversationRef = useRef<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Typing is never blocked during a turn (a turn can run for many minutes);
  // only submitting is, in handleSubmit.
  useRefocusAfterTurn(isSending, textareaRef);

  const enabledToolSet = useMemo(
    () => new Set(toolPreferences.enabledTools ?? []),
    [toolPreferences.enabledTools]
  );

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${textareaRef.current.scrollHeight}px`;
    }
  }, [input]);

  useEffect(() => {
    if (!conversationId) {
      lastAppliedToolPreferenceConversationRef.current = null;
      const defaults = defaultToolPreferences();
      setToolPreferences(defaults);
      toolPreferencesRef.current = defaults;
      return;
    }
    if (lastAppliedToolPreferenceConversationRef.current === conversationId) {
      return;
    }

    const activeConversation = conversations.find(
      (conversation) => conversation.id === conversationId
    );
    if (!activeConversation) {
      return;
    }

    const spaceId = activeConversation?.spaceId;
    if (!spaceId) {
      const defaults = defaultToolPreferences();
      setToolPreferences(defaults);
      toolPreferencesRef.current = defaults;
      lastAppliedToolPreferenceConversationRef.current = conversationId;
      return;
    }

    const space = spaces.find((item) => item.id === spaceId);
    if (!space) {
      return;
    }

    const next = parseToolPreferences(space.toolPreferencesJson);
    setToolPreferences(next);
    toolPreferencesRef.current = next;

    lastAppliedToolPreferenceConversationRef.current = conversationId;
  }, [conversationId, conversations, spaces]);

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
  }, [conversationId, applyFocusDocuments]);

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
    canCompact,
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
        if (conversationId) onCompact(conversationId);
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

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isSending || !conversationId) return;
    // Block submit while warming up or before chat model downloads.
    if (useModelWarmupStore.getState().chat.phase === 'started') return;
    if (isChatUnavailable) return;

    const message = input.trim();

    // A message that is nothing but a command runs the command instead of being
    // sent. /compact was the first of these — a bare regex here that nothing on
    // screen mentioned — and every command in the menu now comes through the
    // same door, typed or picked.
    const typedCommand = parseSlashSubmission(message);
    if (typedCommand) {
      setInput('');
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
      }
      runSlashCommand(typedCommand);
      return;
    }
    // Compacting rewrites the context the next turn reads; the draft keeps
    // until it is done.
    if (isCompacting) return;

    // Staged files join the conversation *with* this message: import them
    // first so the turn can already draw on them, and let the message carry
    // their names and ids. The names draw the chips; the ids are what makes
    // the turn actually read the files, so a send that has one without the
    // other is a file the answer will not have seen. A total import failure
    // aborts the send with the draft intact — sending without the files would
    // answer the wrong question.
    let attachmentNames: string[] | undefined;
    let attachmentDocumentIds: string[] | undefined;
    if (attachments.staged.length > 0) {
      const imported = await attachments.importStaged();
      if (imported === null) return;
      if (imported.names.length > 0) attachmentNames = imported.names;
      if (imported.documentIds.length > 0) attachmentDocumentIds = imported.documentIds;
    }

    setInput('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }

    await sendMessage(
      message,
      conversationId,
      resolveEffectiveToolPreferences(),
      attachmentNames,
      attachmentDocumentIds
    );
  };

  // `@` may only ever offer documents this conversation can read — its own
  // space and its own attachments, never the sidebar's selection, which is a
  // different chat's business.
  const { documents: mentionDocuments, isLoading: isLoadingMentions } = useSpaceDocuments(
    conversationSpaceId,
    conversationId,
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

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
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

  // Deep links and failed regenerations hand the composer its text this way.
  useEffect(() => {
    if (!composerDraft) return;
    setInput(composerDraft);
    setComposerDraft(null);
    textareaRef.current?.focus();
  }, [composerDraft, setComposerDraft]);

  /** Put a question in the draft, ready to edit or send. */
  const fill = useCallback((question: string) => {
    setInput(question);
    textareaRef.current?.focus();
  }, []);

  return {
    input,
    setInput,
    fill,
    textareaRef,
    placeholder,
    focusLabel,
    toolPreferences,
    resolveEffectiveToolPreferences,
    customTools,
    enabledToolSet,
    enabledCustomToolNames,
    turnMode,
    composerMode,
    focusDocuments,
    applyFocusDocuments,
    isControlsOpen,
    setIsControlsOpen,
    handleTurnModeChange,
    toggleKnowledgeBase,
    toggleWebTools,
    toggleWikiTools,
    toggleDeepResearch,
    toggleCustomTool,
    removeMode,
    suggest,
    handleKeyDown,
    handleSubmit,
  };
}

export type ComposerState = ReturnType<typeof useComposer>;
