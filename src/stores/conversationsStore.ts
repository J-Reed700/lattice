import { createContext, createElement, useContext, useLayoutEffect, useRef, type ReactNode } from 'react';

import { useSyncExternalStoreWithSelector } from 'use-sync-external-store/with-selector';
import { createStore, type StoreApi } from 'zustand/vanilla';

import { useConversationsController } from '../hooks/useConversationsController';

import type { ConversationsState } from './conversationsStore.types';

export type { ConversationFilterMode } from './conversationsStore.types';

type ConversationsSnapshot = StoreApi<ConversationsState>;
const ConversationsContext = createContext<ConversationsSnapshot | null>(null);

export function ConversationsProvider({ children }: { children: ReactNode }) {
  const controller = useConversationsController();
  const storeRef = useRef<ConversationsSnapshot | null>(null);
  if (!storeRef.current) storeRef.current = createStore<ConversationsState>(() => controller);

  // Keep the context value stable. Consumers subscribe to the external store
  // with a selector, so a streaming text change only wakes consumers whose
  // selected value actually changed.
  useLayoutEffect(() => {
    storeRef.current?.setState(controller, true);
  }, [controller]);

  return createElement(ConversationsContext.Provider, { value: storeRef.current }, children);
}

function useStore(): ConversationsSnapshot {
  const store = useContext(ConversationsContext);
  if (!store) throw new Error('useConversationsStore must be used within ConversationsProvider.');
  return store;
}

export function useConversationsStore(): ConversationsState;
export function useConversationsStore<T>(selector: (_state: ConversationsState) => T): T;
export function useConversationsStore<T>(
  selector?: (_state: ConversationsState) => T
): ConversationsState | T {
  const store = useStore();
  return useSyncExternalStoreWithSelector(
    store.subscribe,
    store.getState,
    store.getState,
    selector ?? ((state: ConversationsState) => state),
    Object.is
  );
}
