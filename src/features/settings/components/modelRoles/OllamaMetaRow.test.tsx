import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { ActiveModels } from '@/lib/bindings';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { AppSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';

import { OllamaMetaRow } from './OllamaMetaRow';

const h = vi.hoisted(() => ({
  settings: undefined as AppSettings | undefined,
  roles: undefined as unknown as { models: DownloadedModel[]; isLoading: boolean; activeModels: ActiveModels | null },
}));

vi.mock('@/features/settings/hooks/useSettingsQuery', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/settings/hooks/useSettingsQuery')>()),
  useSettingsQuery: () => ({ data: h.settings }),
}));
vi.mock('./ModelRolesContext', () => ({ useModelRoles: () => h.roles }));
vi.mock('../AITab/VisionDeclarationEditor', () => ({ ModelVisionDeclaration: () => null }));
vi.mock('./LearningWindowNotice', () => ({ LearningWindowNotice: () => null }));
vi.mock('./ModelWindowControls', () => ({ ModelWindowControls: () => null }));

const TAG = 'qwen3:8b';

const connectionRow = (overrides: Partial<DownloadedModel> = {}): DownloadedModel => ({
  id: 'ollama',
  model_id: '__ollama_server__',
  model_name: 'Ollama',
  file_path: '',
  file_size_bytes: 0,
  model_type: 'chat',
  downloaded_at: '2026-10-01T00:00:00Z',
  last_used_at: null,
  use_count: 0,
  is_active_for_chat: false,
  is_active_for_embedding: false,
  is_active_for_utility: true,
  backend: 'ollama',
  ...overrides,
});

const active = (patch: Partial<ActiveModels>): ActiveModels => ({
  chat: [], chat_unset: null, utility: [], llama_cpp: null, ollama: { kind: 'ollama', model: TAG }, ollama_utility_tag: TAG,
  ...patch,
});

const renderRow = (activeModels: ActiveModels | null, row = connectionRow()) => {
  h.roles = { models: [row], isLoading: false, activeModels };
  return render(<TooltipProvider><OllamaMetaRow /></TooltipProvider>);
};

describe('OllamaMetaRow utility status', () => {
  beforeEach(() => {
    h.settings = makeAppSettings();
    h.settings.llm.provider = 'auto';
    h.settings.llm.model = TAG;
  });

  it('says what utility work tries first when this connection comes later', () => {
    renderRow(active({
      utility: [{ kind: 'llama_cpp', model: TAG }, { kind: 'ollama', model: TAG }],
    }));
    expect(screen.getByRole('status')).toHaveTextContent(`Utility work tries llama.cpp · ${TAG} first, then this connection.`);
    expect(screen.getByRole('button', { name: 'Change chat provider' })).toBeInTheDocument();
  });

  it('says where utility work runs instead when this connection is not tried', () => {
    renderRow(active({ utility: [{ kind: 'llama_cpp', model: 'served.gguf' }] }));
    expect(screen.getByRole('status')).toHaveTextContent('Utility work runs on llama.cpp · served.gguf instead of this connection.');
  });

  it('says the connection is unused while the chat provider rules it out, with no model to name', () => {
    h.settings!.llm.provider = 'local';
    renderRow(active({ utility: [] }));
    expect(screen.getByRole('status')).toHaveTextContent(
      'Utility work does not run on this connection while the chat provider is Local.',
    );
  });

  it('says nothing when this connection is what utility work tries first, or does not hold Utility', () => {
    const first = renderRow(active({ utility: [{ kind: 'ollama', model: TAG }] }));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    first.unmount();
    renderRow(active({ utility: [] }), connectionRow({ is_active_for_utility: false }));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('shows the utility tag the backend asks for', () => {
    renderRow(active({ utility: [{ kind: 'ollama', model: 'tiny:1b' }], ollama_utility_tag: 'tiny:1b' }));
    expect(screen.getByText(`Chat · ${TAG} · Utility · tiny:1b`)).toBeInTheDocument();
  });
});
