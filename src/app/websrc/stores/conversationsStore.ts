import { createContext, createElement, useContext, type ReactNode } from 'react';

import { useConversationsController } from '../hooks/useConversationsController';

import type { ConversationsState } from './conversationsStore.types';

export type { ConversationFilterMode } from './conversationsStore.types';

const ConversationsContext = createContext<ConversationsState | null>(null);

export function ConversationsProvider({ children }: { children: ReactNode }) {
  const controller = useConversationsController();
  return createElement(ConversationsContext.Provider, { value: controller }, children);
}

export function useConversationsStore(): ConversationsState;
export function useConversationsStore<T>(selector: (_state: ConversationsState) => T): T;
export function useConversationsStore<T>(
  selector?: (_state: ConversationsState) => T
): ConversationsState | T {
  const state = useContext(ConversationsContext);
  if (!state) {
    throw new Error('useConversationsStore must be used within ConversationsProvider.');
  }
  return selector ? selector(state) : state;
}
