import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { WEB_TOOL_NAMES, WIKI_TOOL_NAMES } from '@/features/chat/components/composer/toolNames';
import {
  defaultToolPreferences,
  normalizeTurnMode,
  parseToolPreferences,
  setToolNames,
} from '@/features/chat/components/composer/toolPreferences';
import { DEEP_RESEARCH_WARNING_MESSAGE, type TurnMode } from '@/features/chat/components/ComposerControls';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import type { CustomToolSettings, SpaceDocument, ToolPreferences } from '@/types';

/**
 * The switches a turn goes out with and the documents it is pinned to: loaded
 * from the conversation's space when the chat opens, flipped by the gear
 * popover, the chips and the slash commands, and resolved for each send.
 */
export function useComposerToolPreferences({
  conversationId,
  conversations,
  spaces,
  customTools,
}: {
  conversationId: string | null;
  conversations: ConversationsState['conversations'];
  spaces: ConversationsState['spaces'];
  customTools: CustomToolSettings[];
}) {
  const [toolPreferences, setToolPreferences] = useState<ToolPreferences>(defaultToolPreferences);
  const [focusDocuments, setFocusDocuments] = useState<SpaceDocument[]>([]);
  const toolPreferencesRef = useRef<ToolPreferences>(toolPreferences);
  const lastAppliedToolPreferenceConversationRef = useRef<string | null>(null);

  const enabledToolSet = useMemo(
    () => new Set(toolPreferences.enabledTools ?? []),
    [toolPreferences.enabledTools]
  );

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

  return {
    toolPreferences,
    focusDocuments,
    enabledToolSet,
    applyFocusDocuments,
    toggleKnowledgeBase,
    toggleWebTools,
    toggleWikiTools,
    toggleDeepResearch,
    toggleCustomTool,
    handleTurnModeChange,
    resolveEffectiveToolPreferences,
    turnMode,
    wikipediaEnabled,
    enabledCustomToolNames,
  };
}
