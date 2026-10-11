import { describe, expect, it } from 'vitest';

import type { OptimisticMessage } from '@/types/conversation';

import {
  MAX_UNSAVED_PROMPTS,
  MAX_UNSAVED_PROMPT_BYTES,
  prepareOptimisticMessages,
  persistedRequestIds,
} from './optimisticMessages';

const failedPrompt = (id: number, content = `Prompt ${id}`): OptimisticMessage => ({
  tempId: `failed-${id}`,
  requestId: `request-${id}`,
  conversationId: 'chat',
  role: 'user',
  content,
  status: 'failed',
  createdAt: '2026-09-30T00:00:00.000Z',
});

describe('unsaved conversation prompts', () => {
  it('refuses a new prompt at capacity without evicting the existing text', () => {
    const existing = new Map(Array.from({ length: MAX_UNSAVED_PROMPTS }, (_, i) => {
      const message = failedPrompt(i);
      return [message.tempId, message] as const;
    }));

    expect(() => prepareOptimisticMessages(existing, 'chat', 'One more')).toThrow(/Retry or dismiss/);
    expect([...existing.values()].map(message => message.content)).toEqual(
      Array.from({ length: MAX_UNSAVED_PROMPTS }, (_, i) => `Prompt ${i}`)
    );
  });

  it('keeps a same-text failure when a new message has different send intent', () => {
    const previous = failedPrompt(1, 'Retry me');
    const next = prepareOptimisticMessages(new Map([[previous.tempId, previous]]), 'chat', 'Retry me');

    expect(next.get(previous.tempId)).toEqual(previous);
  });

  it('replaces only the explicitly retried failed prompt, even at capacity', () => {
    const existing = new Map(Array.from({ length: MAX_UNSAVED_PROMPTS }, (_, i) => {
      const message = failedPrompt(i, i === 0 ? 'Retry this' : `Prompt ${i}`);
      return [message.tempId, message] as const;
    }));
    const next = prepareOptimisticMessages(existing, 'chat', 'Retry this', 'failed-0');

    expect(next.size).toBe(MAX_UNSAVED_PROMPTS - 1);
    expect(next.has('failed-0')).toBe(false);
  });

  it('does not reconcile against an older saved question with identical text', () => {
    const old = failedPrompt(1, 'Same wording');
    const persisted = persistedRequestIds([{
      id: 'old-user', conversationId: 'chat', role: 'user', content: 'Same wording',
      tokens: 2, status: 'completed', createdAt: '2026-09-29T00:00:00.000Z',
      metadata: JSON.stringify({ requestId: 'older-request' }),
    }]);

    expect(persisted.has(old.requestId!)).toBe(false);
  });

  it('rejects a prompt larger than the unsaved text budget without changing stored messages', () => {
    const old = failedPrompt(1, 'Keep this text');
    const existing = new Map([[old.tempId, old]]);

    expect(() => prepareOptimisticMessages(existing, 'chat', 'x'.repeat(MAX_UNSAVED_PROMPT_BYTES / 2 + 1)))
      .toThrow(/shorten this message/);
    expect(existing.get(old.tempId)).toEqual(old);
  });
});
