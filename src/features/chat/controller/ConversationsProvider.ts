import { createElement, type ReactNode } from 'react';

import { useConversationsController } from '@/features/chat/controller/useConversationsController';
import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';

export function ConversationsProvider({ children }: { children: ReactNode }) {
  const controller = useConversationsController();
  return createElement(ConversationSnapshotProvider, { value: controller }, children);
}
