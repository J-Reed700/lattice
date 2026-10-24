import { beforeEach, describe, expect, it } from 'vitest';

import { COMMAND_PLUGINS } from '@/shared/ipc/routes.generated';
import { mockIPC } from '@/tests/setup';

import { tagsApi } from './client';

const lastCall = () => mockIPC.mock.calls.at(-1) as [string, Record<string, unknown>];

beforeEach(() => { mockIPC.mockReset(); mockIPC.mockResolvedValue([]); });

describe('tagsApi', () => {
  it('sends every method to a registered command through the plugin that owns it', async () => {
    for (const [name, call] of Object.entries(tagsApi) as [string, (...args: unknown[]) => Promise<unknown>][]) {
      mockIPC.mockClear();
      await call({ documentId: 'doc-1', tagId: 't1' }, ['a']);
      const [route] = lastCall();
      const [, plugin, command] = /^plugin:([^|]+)\|(.+)$/.exec(route) ?? [];
      expect(COMMAND_PLUGINS[command as keyof typeof COMMAND_PLUGINS], `${name} → ${route}`).toBe(plugin);
    }
  });

  it('lists every tag with an empty description', async () => {
    mockIPC.mockResolvedValueOnce([{ id: 't1', name: 'baking', count: 3 }]);
    await expect(tagsApi.listAllTags()).resolves.toEqual({ ok: true, data: { tags: [{ id: 't1', name: 'baking', count: 3, description: null }] } });
  });

  it('returns a document’s tags with its id', async () => {
    mockIPC.mockResolvedValueOnce([{ id: 't1', name: 'baking' }]);
    await expect(tagsApi.getDocumentTags({ documentId: 'doc-1' })).resolves.toEqual({ ok: true, data: { documentId: 'doc-1', tags: [{ id: 't1', name: 'baking' }] } });
    expect(lastCall()[1]).toEqual({ documentId: 'doc-1' });
  });

  it('sends snake_case requests to the backend', async () => {
    await tagsApi.removeTagFromDocument({ documentId: 'doc-1', tagId: 't1' });
    expect(lastCall()[1]).toEqual({ request: { document_id: 'doc-1', tag_id: 't1' } });
    await tagsApi.applyTags('doc-1', ['bread', 'rye']);
    expect(lastCall()[1]).toEqual({ request: { document_id: 'doc-1', tag_names: ['bread', 'rye'] } });
    await tagsApi.generateTagsForDocument('doc-1');
    expect(lastCall()[1]).toEqual({ request: { document_id: 'doc-1', max_tags: 5 } });
  });

  it('passes failures through', async () => {
    mockIPC.mockRejectedValueOnce(new Error('index locked'));
    await expect(tagsApi.listAllTags()).resolves.toMatchObject({ ok: false, error: 'index locked' });
    mockIPC.mockRejectedValueOnce(new Error('index locked'));
    await expect(tagsApi.getDocumentTags({ documentId: 'doc-1' })).resolves.toMatchObject({ ok: false, error: 'index locked' });
  });
});
