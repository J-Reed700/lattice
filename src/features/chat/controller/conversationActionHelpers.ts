import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';

import type { ConversationLifecycleRegistry } from './lifecycleRegistry';
import type { QueryClient } from '@tanstack/react-query';

export interface ConversationActionsDependencies {
  queryClient: QueryClient;
  addRequestedId: (
    field: 'requestedLinkedConversationIds' | 'requestedWebSourceConversationIds' | 'requestedMembershipDocumentIds',
    id: string
  ) => void;
  lifecycle: ConversationLifecycleRegistry;
}


export const setUiError = (error: unknown): void => {
  conversationUiStore.setState({ error: error instanceof Error ? error.message : String(error) });
};

export function restoreMissingItem<T extends { id: string }>(current: T[] | undefined, previous: T[], id: string): T[] | undefined {
  const targetIndex = previous.findIndex(item => item.id === id);
  const target = previous[targetIndex];
  if (!target || current?.some(item => item.id === id)) return current;

  const restored = [...(current ?? [])];
  let insertionIndex = -1;
  // Use surviving history as anchors so concurrent deletions do not move the
  // restored item past messages that followed it in the original sequence.
  for (let index = targetIndex + 1; index < previous.length; index += 1) {
    const successorIndex = restored.findIndex(item => item.id === previous[index].id);
    if (successorIndex >= 0) {
      insertionIndex = successorIndex;
      break;
    }
  }
  if (insertionIndex < 0) {
    for (let index = targetIndex - 1; index >= 0; index -= 1) {
      const predecessorIndex = restored.findIndex(item => item.id === previous[index].id);
      if (predecessorIndex >= 0) {
        insertionIndex = predecessorIndex + 1;
        break;
      }
    }
  }
  if (insertionIndex < 0) insertionIndex = Math.min(targetIndex, restored.length);
  restored.splice(insertionIndex, 0, target);
  return restored;
}

export const forgetRequestedConversation = (id: string): void => {
  conversationUiStore.setState(state => {
    const linked = new Set(state.requestedLinkedConversationIds);
    const web = new Set(state.requestedWebSourceConversationIds);
    linked.delete(id);
    web.delete(id);
    const optimisticMessages = new Map([...state.optimisticMessages].filter(([, message]) => message.conversationId !== id));
    const inFlightGenerations = new Map(state.inFlightGenerations);
    const liveRetrieval = new Map(state.liveRetrieval);
    const liveSteps = new Map(state.liveSteps);
    const liveModel = new Map(state.liveModel);
    inFlightGenerations.delete(id);
    liveRetrieval.delete(id);
    liveSteps.delete(id);
    liveModel.delete(id);
    return { requestedLinkedConversationIds: linked, requestedWebSourceConversationIds: web, optimisticMessages, inFlightGenerations, liveRetrieval, liveSteps, liveModel };
  });
};
