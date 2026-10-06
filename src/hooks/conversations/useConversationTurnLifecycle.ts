import { useCallback } from 'react';

import { listen } from '@tauri-apps/api/event';

import { VaultAPI } from '@/lib/api';
import type { ChatResponse, ChatStreamEventDto } from '@/lib/bindings';
import type { GenerationOutcome } from '@/stores/conversationsStore.types';
import { conversationUiStore } from '@/stores/conversationUiStore';
import { currentExplorerFocus, explorerStore } from '@/stores/explorerStore';
import type { ApiResult } from '@/types';
import { ErrorCode } from '@/types/api/errorCodes';
import type {
  Conversation, ConversationMessage, OptimisticMessage, SourceWithMetadata, ToolPreferences,
} from '@/types/conversation';
import { RetrievalTraceSchema, TurnStepSchema } from '@/types/conversation';
import { ChatStreamStatus } from '@/types/events';
import { createDefaultConversationTitle } from '@/utils/conversationTitles';
import { createFrameBatcher } from '@/utils/frameBatcher';

import {
  applyVerificationPatch, isVerificationPending, markVerificationInterrupted, mergeStep,
  parseSources, type VerificationPatch, VERIFICATION_WAIT_MS,
} from './messageMetadata';
import { prepareOptimisticMessages, reconcilePersistedFailedMessages } from './optimisticMessages';
import { conversationKeys } from '../queries/conversationKeys';
import { fetchMessages, toConversationMessage } from '../queries/conversationQueryData';

import type { ConversationLifecycleRegistry } from './lifecycleRegistry';
import type { QueryClient } from '@tanstack/react-query';

const pendingCancellationRequests = new Set<string>();

const setUiError = (error: unknown): void => {
  conversationUiStore.setState({ error: error instanceof Error ? error.message : String(error) });
};

const isUserInitiatedCancellation = (requestId: string, errorCode?: string): boolean =>
  pendingCancellationRequests.has(requestId)
  && (errorCode === ErrorCode.INVALID_STATE || errorCode === ErrorCode.INTERNAL_ERROR);

/**
 * A turn from an Explorer thread carries what the Explorer shows: the open
 * file and the selected lines. Only while the Explorer is on that thread's
 * folder, since the paths are relative to it.
 */
const withExplorerFocus = (
  conversation: Conversation | undefined,
  toolPreferences: ToolPreferences | undefined
): ToolPreferences | undefined => {
  const root = conversation?.explorerRoot;
  if (!root || explorerStore.getState().root?.root !== root) return toolPreferences;
  const focus = currentExplorerFocus();
  if (!focus) return toolPreferences;
  // Every switch is optional on the wire: a turn sent without preferences
  // (a regenerate from the message menu) keeps the backend's defaults.
  return { ...toolPreferences, explorerFocus: focus } as ToolPreferences;
};

interface TurnLifecycleDependencies {
  queryClient: QueryClient;
  conversations: Conversation[];
  invalidateLists: () => Promise<void>;
  lifecycle: ConversationLifecycleRegistry;
  addRequestedId: (field: 'requestedLinkedConversationIds' | 'requestedWebSourceConversationIds' | 'requestedMembershipDocumentIds', id: string) => void;
}

export function useConversationTurnLifecycle({ queryClient, conversations, invalidateLists, addRequestedId, lifecycle }: TurnLifecycleDependencies) {
  const runGeneration = useCallback(async (options: {
    conversationId: string;
    content: string;
    showUserBubble: boolean;
    invoke: (_requestId: string) => Promise<ApiResult<ChatResponse>>;
    onFailure?: (_error: string) => void;
    retryContext?: OptimisticMessage['retryContext'];
    replacesFailedTempId?: string;
  }): Promise<GenerationOutcome> => {
    const {
      conversationId: requestConversationId, content, showUserBubble, invoke, onFailure,
      retryContext, replacesFailedTempId,
    } = options;
    const state = conversationUiStore.getState();
    const ownsComposer = () => {
      const activeId = conversationUiStore.getState().activeConversationId;
      return activeId === requestConversationId || (activeId === null && state.activeConversationId === null);
    };
    if (state.inFlightGenerations.has(requestConversationId)) {
      setUiError('A response is already being generated for this conversation.');
      return 'busy';
    }

    let preparedMessages = state.optimisticMessages;
    if (showUserBubble) {
      try {
        preparedMessages = prepareOptimisticMessages(
          state.optimisticMessages, requestConversationId, content, replacesFailedTempId
        );
      } catch (error) {
        setUiError(error);
        // The submitter clears its input while awaiting us; return this unsent text.
        if (ownsComposer()) {
          conversationUiStore.setState({ composerDraft: content });
        }
        return 'failed';
      }
    }
    const requestId = crypto.randomUUID();
    let lifecycleCancelled = false;
    const ownsTurn = () => !lifecycleCancelled
      && conversationUiStore.getState().inFlightGenerations.get(requestConversationId) === requestId;
    const userTempId = crypto.randomUUID();
    const assistantTempId = crypto.randomUUID();
    const now = new Date().toISOString();
    const userMessage: OptimisticMessage = {
      tempId: userTempId,
      requestId,
      conversationId: requestConversationId,
      content,
      role: 'user',
      status: 'pending',
      createdAt: now,
      ...(retryContext ? { retryContext } : {}),
    };
    const assistantMessage: OptimisticMessage = {
      tempId: assistantTempId,
      conversationId: requestConversationId,
      content: '',
      role: 'assistant',
      status: 'pending',
      createdAt: now,
    };
    conversationUiStore.setState(current => {
      const optimisticMessages = new Map(preparedMessages);
      if (showUserBubble) optimisticMessages.set(userTempId, userMessage);
      optimisticMessages.set(assistantTempId, assistantMessage);
      const inFlightGenerations = new Map(current.inFlightGenerations);
      inFlightGenerations.set(requestConversationId, requestId);
      const liveRetrieval = new Map(current.liveRetrieval);
      liveRetrieval.delete(requestConversationId);
      const liveSteps = new Map(current.liveSteps);
      liveSteps.delete(requestConversationId);
      return { optimisticMessages, inFlightGenerations, liveRetrieval, liveSteps, error: null };
    });

    const settleOptimisticMessages = (error?: string) => {
      if (!ownsTurn()) return;
      conversationUiStore.setState(current => {
        const optimisticMessages = new Map(current.optimisticMessages);
        if (error && showUserBubble) {
          const failed = optimisticMessages.get(userTempId);
          if (failed) {
            optimisticMessages.set(userTempId, { ...failed, status: 'failed', error });
          }
        } else {
          optimisticMessages.delete(userTempId);
        }
        optimisticMessages.delete(assistantTempId);
        return {
          optimisticMessages,
          ...(error ? { error } : {}),
        };
      });
      if (error) onFailure?.(error);
    };

    let unlisten: (() => void) | undefined;
    let pendingStreamContent = '';
    const flushStreamContent = () => {
      const content = pendingStreamContent;
      pendingStreamContent = '';
      if (!content || lifecycleCancelled) return;
      conversationUiStore.setState(current => {
        const optimisticMessages = new Map(current.optimisticMessages);
        const existing = optimisticMessages.get(assistantTempId);
        if (!existing) return current;
        optimisticMessages.set(assistantTempId, {
          ...existing,
          content: existing.content + content,
        });
        return { optimisticMessages };
      });
    };
    const streamBatcher = createFrameBatcher(flushStreamContent);
    // The answer returns before its grounding check finishes; the check
    // arrives on this same channel afterwards. The listener stays up for it
    // after the turn settles, and only for it.
    let awaitingVerification = false;
    let verificationConversationId = requestConversationId;
    let verificationTimer: ReturnType<typeof setTimeout> | undefined;
    let readyVerification: VerificationPatch | null = null;
    let disposed = false;
    let unregisterLifecycle: (() => void) | undefined;
    const disposeResources = () => {
      disposed = true;
      pendingStreamContent = '';
      streamBatcher.cancel();
      if (verificationTimer) clearTimeout(verificationTimer);
      unlisten?.();
      unlisten = undefined;
    };
    const cancelLifecycle = () => {
      lifecycleCancelled = true;
      disposeResources();
    };
    const stopListening = () => {
      disposeResources();
      unregisterLifecycle?.();
      unregisterLifecycle = undefined;
    };
    unregisterLifecycle = lifecycle.register(requestConversationId, cancelLifecycle);
    try {
      unlisten = await listen<ChatStreamEventDto>('llm-stream', event => {
        const payload = event.payload;
        if (disposed) return;
        // Matched on the request alone: a turn that created its conversation
        // reports the conversation's real id, not the one it was asked for.
        if (payload.requestId !== requestId) return;
        if (!awaitingVerification && !ownsTurn()) return;
        if (payload.status === ChatStreamStatus.Verification && payload.verification) {
          const patch: VerificationPatch = {
            messageId: payload.verification.messageId,
            verification: payload.verification.verification,
            turn: payload.verification.turn ?? undefined,
          };
          readyVerification = patch;
          queryClient.setQueryData<ConversationMessage[]>(
            conversationKeys.messages(payload.conversationId),
            current => (current ? applyVerificationPatch(current, patch) : current)
          );
          if (awaitingVerification) stopListening();
          return;
        }
        if (payload.conversationId !== requestConversationId) return;
        if (payload.done) return;
        // A round can run for minutes without a single character of text. The
        // steps are the only thing distinguishing "still working" from "hung",
        // and unlike the note they replace they are kept: the same list is
        // persisted with the answer.
        if (payload.status === 'step' && payload.step) {
          const parsed = TurnStepSchema.safeParse(payload.step);
          if (parsed.success) {
            conversationUiStore.setState(current => {
              const liveSteps = new Map(current.liveSteps);
              liveSteps.set(
                requestConversationId,
                mergeStep(liveSteps.get(requestConversationId), parsed.data)
              );
              return { liveSteps };
            });
          }
          return;
        }
        // Tool searches can update the initial retrieval trace during generation.
        if (payload.status === 'retrieval' && payload.retrieval) {
          const parsed = RetrievalTraceSchema.safeParse(payload.retrieval);
          if (parsed.success) {
            conversationUiStore.setState(current => {
              const liveRetrieval = new Map(current.liveRetrieval);
              liveRetrieval.set(requestConversationId, parsed.data);
              return { liveRetrieval };
            });
          }
          return;
        }
        // A retry is a step of its own now. It used to arrive as text and
        // overwrite the bubble, so a turn that recovered showed the retry
        // notice where its answer should have been.
        if (!payload.content) return;
        pendingStreamContent += payload.content;
        streamBatcher.schedule();
      });

      if (disposed || !ownsTurn()) return 'cancelled';
      const result = await invoke(requestId);
      if (!ownsTurn()) return 'cancelled';
      // Do not let a fast terminal response drop the final few queued tokens.
      streamBatcher.flush();
      if (!result.ok) {
        if (isUserInitiatedCancellation(requestId, result.details?.code)) {
          const refreshed = await fetchMessages(requestConversationId);
          if (!ownsTurn()) return 'cancelled';
          queryClient.setQueryData(conversationKeys.messages(requestConversationId), refreshed);
          conversationUiStore.setState(current => {
            const optimisticMessages = reconcilePersistedFailedMessages(
              current.optimisticMessages, requestConversationId, refreshed
            );
            return optimisticMessages === current.optimisticMessages ? current : { optimisticMessages };
          });
          settleOptimisticMessages();
          // A stop during retrieval ends the turn before the backend has
          // persisted the question, so the bubble we just dropped was its only
          // copy. Hand it back to the composer rather than lose it.
          const lastUser = [...refreshed].reverse().find(message => message.role === 'user');
          const questionLost = showUserBubble && lastUser?.content !== content;
          conversationUiStore.setState(questionLost && ownsComposer()
            ? { error: null, composerDraft: content }
            : { error: null });
          return 'cancelled';
        }
        const detail: unknown = result.details?.details;
        settleOptimisticMessages(typeof detail === 'string' && detail.trim() ? detail : result.error);
        // A failed provider call can still have persisted the question. Replace
        // only the optimistic copy carrying that exact request id.
        try {
          const refreshed = await fetchMessages(requestConversationId);
          if (ownsTurn()) {
            queryClient.setQueryData(conversationKeys.messages(requestConversationId), refreshed);
            conversationUiStore.setState(current => {
              const optimisticMessages = reconcilePersistedFailedMessages(
                current.optimisticMessages, requestConversationId, refreshed
              );
              return optimisticMessages === current.optimisticMessages ? current : { optimisticMessages };
            });
          }
        } catch { /* Keep the unsaved prompt when its persistence is unknown. */ }
        return 'failed';
      }

      const raw = result.data as typeof result.data & {
        conversation_id?: string;
        sources?: SourceWithMetadata[];
      };
      const responseId = raw.conversationId ?? raw.conversation_id;
      verificationConversationId = responseId ?? requestConversationId;
      if (!responseId) {
        settleOptimisticMessages('Chat response missing conversation ID. Please try again.');
        return 'failed';
      }
      if (responseId !== requestConversationId) {
        unregisterLifecycle?.();
        unregisterLifecycle = lifecycle.register(responseId, cancelLifecycle);
      }
      // The check may have landed while the response was on its way; the
      // response itself was read before it did.
      const persistedMessages = raw.messages.map(toConversationMessage);
      const responseMessages = readyVerification
        ? applyVerificationPatch(persistedMessages, readyVerification)
        : persistedMessages;
      const lastAssistant = [...responseMessages].reverse().find(message => message.role === 'assistant');
      awaitingVerification = isVerificationPending(lastAssistant) && !readyVerification;
      if (lastAssistant && raw.sources?.length && !lastAssistant.sources?.length) {
        lastAssistant.sources = parseSources(raw.sources);
      }
      queryClient.setQueryData(conversationKeys.messages(responseId), responseMessages);
      queryClient.setQueriesData<Conversation[]>({ queryKey: conversationKeys.lists }, current => {
        if (!current) return current;
        const existing = current.find(item => item.id === responseId);
        if (existing) {
          return current.map(item => item.id === responseId
            ? { ...item, updatedAt: new Date().toISOString() }
            : item);
        }
        // A known conversation can be absent because it is a tangent or the
        // list is filtered. Only a newly created id belongs in this fallback.
        if (responseId === requestConversationId) return current;
        return [{
          id: responseId,
          title: createDefaultConversationTitle(),
          updatedAt: new Date().toISOString(),
        }, ...current];
      });
      conversationUiStore.setState(current => {
        const optimisticMessages = new Map(current.optimisticMessages);
        optimisticMessages.delete(userTempId);
        optimisticMessages.delete(assistantTempId);
        // Adopting the id is only right for a turn that created the
        // conversation. Doing it unconditionally yanked the user back to a
        // finished turn they had already navigated away from.
        const created = responseId !== requestConversationId;
        return created
          ? { activeConversationId: responseId, optimisticMessages }
          : { optimisticMessages };
      });
      addRequestedId('requestedLinkedConversationIds', responseId);
      await Promise.all([
        invalidateLists(),
        queryClient.invalidateQueries({ queryKey: conversationKeys.linkedDocuments(responseId) }),
      ]);
      return ownsTurn() ? 'answered' : 'cancelled';
    } catch (error) {
      streamBatcher.flush();
      settleOptimisticMessages(error instanceof Error ? error.message : String(error));
      return 'failed';
    } finally {
      streamBatcher.cancel();
      streamBatcher.flush();
      if (awaitingVerification && unlisten) {
        // A check that never reports must not leave the badge on "Checking…"
        // until the conversation is reopened: mark it interrupted and stop.
        const pendingMessages = conversationKeys.messages(verificationConversationId);
        verificationTimer = setTimeout(() => {
          queryClient.setQueryData<ConversationMessage[]>(pendingMessages, current =>
            current ? markVerificationInterrupted(current) : current
          );
          stopListening();
        }, VERIFICATION_WAIT_MS);
      } else {
        stopListening();
      }
      pendingCancellationRequests.delete(requestId);
      conversationUiStore.setState(current => {
        if (current.inFlightGenerations.get(requestConversationId) !== requestId) return current;
        let optimisticMessages: Map<string, OptimisticMessage> | undefined;
        if (lifecycleCancelled) {
          optimisticMessages = new Map(current.optimisticMessages);
          const interrupted = optimisticMessages.get(userTempId);
          if (showUserBubble && interrupted) {
            optimisticMessages.set(userTempId, {
              ...interrupted,
              status: 'failed',
              error: 'The chat closed before this message finished. Try again to resend it.',
            });
          } else {
            optimisticMessages.delete(userTempId);
          }
          optimisticMessages.delete(assistantTempId);
        }
        const inFlightGenerations = new Map(current.inFlightGenerations);
        inFlightGenerations.delete(requestConversationId);
        const liveRetrieval = new Map(current.liveRetrieval);
        liveRetrieval.delete(requestConversationId);
        const liveSteps = new Map(current.liveSteps);
        liveSteps.delete(requestConversationId);
        return {
          inFlightGenerations, liveRetrieval, liveSteps,
          ...(optimisticMessages ? { optimisticMessages } : {}),
        };
      });
      if (lifecycleCancelled) {
        void queryClient.invalidateQueries({ queryKey: conversationKeys.messages(requestConversationId) });
      }
    }
  }, [addRequestedId, invalidateLists, lifecycle, queryClient]);

  const sendMessage = useCallback(async (
    content: string,
    conversationId?: string | null,
    toolPreferences?: ToolPreferences,
    attachmentNames?: string[],
    attachmentDocumentIds?: string[]
  ) => {
    const state = conversationUiStore.getState();
    // The fallback is a Chat conversation: an Explorer thread is never
    // continued without its folder.
    const requestConversationId = conversationId
      ?? state.activeConversationId
      ?? conversations.find(conversation => !conversation.explorerRoot)?.id
      ?? null;
    if (!requestConversationId) {
      setUiError('No active conversation selected. Create or select a conversation first.');
      return;
    }
    toolPreferences = withExplorerFocus(
      conversations.find(conversation => conversation.id === requestConversationId)
        ?? queryClient.getQueryData<Conversation>(conversationKeys.detail(requestConversationId)),
      toolPreferences
    );
    return runGeneration({
      conversationId: requestConversationId,
      content,
      showUserBubble: true,
      invoke: requestId => VaultAPI.chatWithConversation(
        requestConversationId,
        content,
        toolPreferences,
        requestId,
        attachmentNames,
        attachmentDocumentIds
      ),
      retryContext: {
        ...(toolPreferences ? { toolPreferences } : {}),
        ...(attachmentNames ? { attachmentNames } : {}),
        ...(attachmentDocumentIds ? { attachmentDocumentIds } : {}),
      },
    });
  }, [conversations, queryClient, runGeneration]);

  const retryFailedMessage = useCallback(async (tempId: string) => {
    const failed = conversationUiStore.getState().optimisticMessages.get(tempId);
    if (failed?.role !== 'user' || failed.status !== 'failed') return;
    await runGeneration({
      conversationId: failed.conversationId,
      content: failed.content,
      showUserBubble: true,
      retryContext: failed.retryContext,
      replacesFailedTempId: tempId,
      invoke: requestId => VaultAPI.chatWithConversation(
        failed.conversationId,
        failed.content,
        failed.retryContext?.toolPreferences,
        requestId,
        failed.retryContext?.attachmentNames,
        failed.retryContext?.attachmentDocumentIds
      ),
    });
  }, [runGeneration]);

  /**
   * Re-run the last user message.
   *
   * The backend lifts the question off the thread and re-persists it, so no
   * user bubble is pushed here. If generation fails it hands the question back
   * through `onFailure` so the caller can put it in the composer — a
   * regenerate must never cost the user their question.
   *
   * Returns how the turn ended, so callers announce neither an answer that
   * never arrived nor a failure that never happened.
   */
  const regenerateResponse = useCallback(async (
    conversationId: string,
    toolPreferences?: ToolPreferences
  ): Promise<GenerationOutcome> => {
    const messages = queryClient.getQueryData<ConversationMessage[]>(
      conversationKeys.messages(conversationId)
    ) ?? [];
    const lastUser = [...messages].reverse().find(message => message.role === 'user');
    return runGeneration({
      conversationId,
      content: lastUser?.content ?? '',
      showUserBubble: false,
      invoke: requestId => VaultAPI.regenerateResponse(
        conversationId,
        withExplorerFocus(conversations.find(conversation => conversation.id === conversationId)
          ?? queryClient.getQueryData<Conversation>(conversationKeys.detail(conversationId)), toolPreferences),
        requestId
      ),
      onFailure: () => {
        if (lastUser?.content && conversationUiStore.getState().activeConversationId === conversationId) {
          conversationUiStore.setState({ composerDraft: lastUser.content });
        }
      },
    });
  }, [conversations, queryClient, runGeneration]);

  /**
   * Drop every message after `messageId` (and it too when `inclusive`).
   * Replaces the message cache with what the server says survived.
   */
  const cancelGeneration = useCallback(async (conversationId?: string | null) => {
    const state = conversationUiStore.getState();
    const id = conversationId ?? state.activeConversationId;
    if (!id) return;
    const requestId = state.inFlightGenerations.get(id);
    if (!requestId) return;
    pendingCancellationRequests.add(requestId);
    const result = await VaultAPI.cancelConversationGeneration(id, requestId);
    if (!result.ok) {
      pendingCancellationRequests.delete(requestId);
      setUiError(result.error);
    }
  }, []);


  return { sendMessage, retryFailedMessage, regenerateResponse, cancelGeneration };
}
