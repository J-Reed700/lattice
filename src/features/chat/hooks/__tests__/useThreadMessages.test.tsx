import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { useThreadMessages } from '@/features/chat/hooks/useThreadMessages';

const conversations = [
  { id: 'c1', messages: [{ id: 'm1', conversationId: 'c1', role: 'user', content: 'When was Carthage founded?' }, { id: 'm2', conversationId: 'c1', role: 'assistant', content: 'About 814 BC.' }] },
  { id: 'c2' },
] as never;

const optimistic = new Map([
  ['t1', { tempId: 't1', conversationId: 'c1', role: 'user', content: 'And destroyed?' }],
  ['t2', { tempId: 't2', conversationId: 'temp', role: 'user', content: 'First question' }],
  ['t3', { tempId: 't3', conversationId: 'c9', role: 'user', content: 'Elsewhere' }],
]) as never;

describe('useThreadMessages', () => {
  it('joins saved and pending messages for the open chat', () => {
    const { result } = renderHook(() => useThreadMessages('c1', conversations, optimistic));
    expect(result.current.messages.map((message) => message.content)).toEqual([
      'When was Carthage founded?', 'About 814 BC.', 'And destroyed?', 'First question',
    ]);
    expect(result.current.messageContentById).toEqual(new Map([['m1', 'When was Carthage founded?'], ['m2', 'About 814 BC.']]));
    const [saved, , pending] = result.current.messages;
    expect(result.current.getMessageKey(saved)).toBe('m1');
    expect(result.current.getMessageKey(pending)).toBe('t1');
    expect(result.current.getPersistedMessageId(saved)).toBe('m1');
    expect(result.current.getPersistedMessageId(pending)).toBeNull();
  });

  it('is empty with no chat open, and copes with a chat that has no messages loaded', () => {
    expect(renderHook(() => useThreadMessages(null, conversations, optimistic)).result.current.messages).toEqual([]);
    const { result } = renderHook(() => useThreadMessages('c2', conversations, new Map()));
    expect(result.current.messages).toEqual([]);
  });
});
