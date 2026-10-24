import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { mentionApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue(null); });

describe('mentionApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(mentionApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      await call('arg-1', 'arg-2');
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('names its arguments the way the backend expects', async () => {
    await mentionApi.extractMentions('doc-1', 'Met @Hypatia in Alexandria');
    expect(lastCall()[1]).toEqual({ documentId: 'doc-1', content: 'Met @Hypatia in Alexandria' });
    await mentionApi.searchMentions('hyp', 5);
    expect(lastCall()[1]).toEqual({ query: 'hyp', limit: 5 });
    await mentionApi.getBacklinksForMention('Hypatia');
    expect(lastCall()[1]).toEqual({ mentionName: 'Hypatia' });
    await mentionApi.createMention('Hypatia', 'person');
    expect(lastCall()[1]).toEqual({ name: 'Hypatia', mentionType: 'person' });
    await mentionApi.deleteMention('m1');
    expect(lastCall()[1]).toEqual({ mentionId: 'm1' });
  });
});
