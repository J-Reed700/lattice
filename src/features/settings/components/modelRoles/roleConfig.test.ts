import { describe, expect, it } from 'vitest';

import { roleDescriptor } from '@/features/settings/components/modelRoles/roleConfig';
import type { ActiveModels } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';

describe('role descriptors', () => {
  it('says what the Verifier audits in statements, not claims', () => {
    expect(roleDescriptor('verifier').hint).toBe('Audits every statement of a lesson against its cited passages before it publishes.');
  });
});

describe('the Chat role', () => {
  const chat = roleDescriptor('chat');
  const row = (overrides: Partial<DownloadedModel>): DownloadedModel => ({
    id: 'row-1',
    model_name: 'Row',
    model_id: 'row-model',
    file_path: '/models/row.gguf',
    file_size_bytes: 1024,
    downloaded_at: '2026-10-01T00:00:00Z',
    last_used_at: null,
    use_count: 0,
    is_active_for_chat: false,
    is_active_for_embedding: false,
    is_active_for_utility: false,
    backend: 'local',
    model_type: 'chat',
    ...overrides,
  });
  const active = (chatModels: ActiveModels['chat']): ActiveModels => ({
    chat: chatModels, chat_unset: null, utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null,
  });

  it('is held by the Ollama connection row under Auto when the backend runs a chat on its tag', () => {
    const settings = makeAppSettings().llm;
    settings.provider = 'auto';
    settings.model = 'qwen3:8b';
    const connection = row({ model_id: '__ollama_server__', backend: 'ollama' });
    expect(chat.isActive(connection, settings, active([{ kind: 'ollama', model: 'qwen3:8b' }]))).toBe(true);
    expect(chat.isActive(connection, settings, active([{ kind: 'local', model_id: 'row-model' }]))).toBe(false);
    expect(chat.isActive(connection, settings, null)).toBe(false);
  });

  it('is not held by a local row flagged for chat when the backend runs a chat on another model', () => {
    const settings = makeAppSettings().llm;
    const flagged = row({ is_active_for_chat: true });
    expect(chat.isActive(flagged, settings, active([{ kind: 'llama_cpp', model: 'served.gguf' }]))).toBe(false);
    expect(chat.isActive(flagged, settings, active([{ kind: 'local', model_id: 'row-model' }]))).toBe(true);
  });
});
