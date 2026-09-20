import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '../api';

// The suite mocks `src/lib/api` globally so components get a stubbed VaultAPI.
// This file is about the real routing table, so it opts back out. `vi.unmock`
// is hoisted above the import, so the real module is what loads.
vi.unmock('../api');

const mockInvoke = vi.mocked(invoke);

describe('VaultAPI.getConversationMemory', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('routes to the conversation domain so the command is not rejected as unregistered', async () => {
    // A missing COMMAND_DOMAIN_MAP entry throws before the IPC call, so this
    // both proves the route exists and pins the domain it points at.
    mockInvoke.mockResolvedValue({ conversationId: 'conv-1' });

    const result = await VaultAPI.getConversationMemory('conv-1');

    expect(result.ok).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('plugin:conversation|get_conversation_memory', {
      request: { conversationId: 'conv-1', includeHistory: null },
    });
  });

  it('sends includeHistory only when the caller asked for superseded items', async () => {
    mockInvoke.mockResolvedValue({ conversationId: 'conv-1' });

    await VaultAPI.getConversationMemory('conv-1', true);

    expect(mockInvoke).toHaveBeenLastCalledWith(
      'plugin:conversation|get_conversation_memory',
      { request: { conversationId: 'conv-1', includeHistory: true } }
    );
  });

  it('returns a failed ApiResult rather than throwing when the backend rejects', async () => {
    // The memory panel renders the error inline, so a rejection must come back
    // as data and not as an exception through React.
    mockInvoke.mockRejectedValue(new Error('memory store unavailable'));

    const result = await VaultAPI.getConversationMemory('conv-1');

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain('memory store unavailable');
    }
  });
});
