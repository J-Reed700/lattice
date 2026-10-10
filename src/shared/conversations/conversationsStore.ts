import { createContext, createElement, useContext, useLayoutEffect, useRef, type ReactNode } from 'react';

import { useSyncExternalStoreWithSelector } from 'use-sync-external-store/with-selector';
import { createStore, type StoreApi } from 'zustand/vanilla';

import type { ConversationsState } from './conversationsStore.types';

export type { ConversationFilterMode } from './conversationsStore.types';

/**
 * The conversation session every surface reads: the conversations, the
 * selected space and the turn in flight. Chat's controller provides it at the
 * root (`ConversationsProvider`); Journal, Spaces and the others read it here
 * without importing Chat.
 */
type ConversationsSnapshot = StoreApi<ConversationsState>;
const ConversationsContext = createContext<ConversationsSnapshot | null>(null);

/** A transcript can project the shared controller onto a different conversation
 * without selecting it globally or starting a second generation lifecycle. */
export function ConversationSnapshotProvider({ value, children }: { value: ConversationsState; children?: ReactNode }) {
  const storeRef = useRef<ConversationsSnapshot | null>(null);
  if (!storeRef.current) storeRef.current = createStore<ConversationsState>(() => value);

  // Keep the context value stable. Consumers subscribe to the external store
  // with a selector, so a streaming text change only wakes consumers whose
  // selected value actually changed.
  useLayoutEffect(() => {
    storeRef.current?.setState(value, true);
  }, [value]);

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
