import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ModelPickerPopover } from '../ModelPickerPopover';

import type { DownloadedModel } from '../../../types/downloadedModels';

const models = vi.hoisted(() => ({ current: [] as DownloadedModel[] }));

vi.mock('@/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ downloadedModels: models.current }),
}));

const model = (overrides: Partial<DownloadedModel>): DownloadedModel => ({
  id: 'row-1',
  model_name: 'Llama 3.2 3B',
  model_id: 'llama-3.2-3b',
  file_path: '/models/llama.gguf',
  file_size_bytes: 1,
  downloaded_at: '2026-01-01T00:00:00Z',
  last_used_at: null,
  use_count: 0,
  is_active_for_chat: false,
  is_active_for_embedding: false,
  is_active_for_utility: false,
  backend: 'local',
  model_type: 'language_model',
  ...overrides,
});

const open = async () => {
  render(
    <ModelPickerPopover activeModelId="llama-3.2-3b" onSelect={vi.fn()}>
      <button type="button">Try with…</button>
    </ModelPickerPopover>
  );
  await userEvent.click(screen.getByText('Try with…'));
};

describe('ModelPickerPopover', () => {
  it('lists only chat models', async () => {
    models.current = [
      model({ id: 'a' }),
      model({ id: 'b', model_name: 'BGE small', model_type: 'text_embeddings' }),
    ];
    await open();

    expect(screen.getByText('Llama 3.2 3B')).toBeInTheDocument();
    expect(screen.queryByText('BGE small')).not.toBeInTheDocument();
  });

  it('groups by backend and marks the active row', async () => {
    models.current = [
      model({ id: 'a', is_active_for_chat: true }),
      model({
        id: 'b',
        model_name: 'mistral:latest',
        model_id: 'mistral:latest',
        backend: 'ollama',
      }),
    ];
    await open();

    expect(screen.getByText('Local')).toBeInTheDocument();
    expect(screen.getByText('Ollama')).toBeInTheDocument();
    // The active row is the one the label points at.
    const activeRow = screen.getByText('Llama 3.2 3B').closest('button');
    expect(activeRow?.querySelector('svg')).not.toBeNull();
  });

  it('omits group labels when only one backend is present', async () => {
    models.current = [model({ id: 'a' })];
    await open();

    expect(screen.queryByText('Local')).not.toBeInTheDocument();
    expect(screen.queryByText('Ollama')).not.toBeInTheDocument();
  });

  it('says so plainly when nothing else is installed', async () => {
    models.current = [];
    await open();

    expect(screen.getByText('No other models installed.')).toBeInTheDocument();
  });
});
