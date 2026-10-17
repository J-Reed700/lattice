import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';

import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';


const assignRole = vi.fn();
const context = vi.hoisted(() => ({ llmSettings: undefined as LLMSettings | undefined }));

vi.mock('./ModelRolesContext', () => ({
  useModelRoles: () => ({ refresh: vi.fn(), assignRole, llmSettings: context.llmSettings }),
}));

const embeddingRole = ROLES.find((role) => role.id === 'embedding')!;

function embeddingModel(active: boolean): DownloadedModel {
  return {
    id: 'm1',
    model_name: 'Qwen3 Embedding 0.6B',
    model_id: 'qwen3-embedding-0.6b',
    file_path: '/models/qwen3-embedding-0.6b',
    file_size_bytes: 1_200_000_000,
    model_type: embeddingRole.allowedTypes[0],
    downloaded_at: '2026-09-01T00:00:00Z',
    last_used_at: null,
    use_count: 0,
    is_active_for_chat: false,
    is_active_for_embedding: active,
    is_active_for_utility: false,
    backend: 'local',
  };
}

describe('RoleButton', () => {
  beforeEach(() => assignRole.mockReset());

  // A model that holds a role cannot be deleted. With the only embedding model
  // active and its button disabled, there was no way to remove it.
  it('gives up a role the model already holds', async () => {
    render(<RoleButton model={embeddingModel(true)} role={embeddingRole} />);

    await userEvent.click(screen.getByRole('button', { name: embeddingRole.label }));

    expect(assignRole).toHaveBeenCalledWith(null, 'embedding');
  });

  it('takes a role the model does not hold', async () => {
    render(<RoleButton model={embeddingModel(false)} role={embeddingRole} />);

    await userEvent.click(screen.getByRole('button', { name: embeddingRole.label }));

    expect(assignRole).toHaveBeenCalledWith('qwen3-embedding-0.6b', 'embedding');
  });

  describe('Learning', () => {
    const learningRole = ROLES.find((role) => role.id === 'learning')!;
    const chatModel = (overrides: Partial<DownloadedModel> = {}): DownloadedModel => ({
      ...embeddingModel(false), model_name: 'Qwen3.5-27B', model_id: 'qwen3.5-27b', model_type: 'chat', ...overrides,
    });

    beforeEach(() => {
      context.llmSettings = makeAppSettings().llm;
    });

    it('is the fourth role, offered to chat models and not to embedding models', () => {
      expect(ROLES.map((role) => role.id)).toEqual(['chat', 'utility', 'embedding', 'learning']);
      const { unmount } = render(<RoleButton model={chatModel()} role={learningRole} />);
      expect(screen.getByRole('button', { name: 'Learning' })).toHaveAttribute('aria-pressed', 'false');
      unmount();
      render(<RoleButton model={embeddingModel(false)} role={learningRole} />);
      expect(screen.queryByRole('button', { name: 'Learning' })).toBeNull();
    });

    it('reads its state from the model reference in Settings, not from a row flag', async () => {
      context.llmSettings!.learningModel = { kind: 'local', model_id: 'qwen3.5-27b' };
      render(<RoleButton model={chatModel()} role={learningRole} />);
      const button = screen.getByRole('button', { name: 'Learning' });
      expect(button).toHaveAttribute('aria-pressed', 'true');
      await userEvent.click(button);
      expect(assignRole).toHaveBeenCalledWith(null, 'learning');
    });

    it('takes the role for a row that does not hold it', async () => {
      context.llmSettings!.learningModel = { kind: 'local', model_id: 'other' };
      render(<RoleButton model={chatModel()} role={learningRole} />);
      await userEvent.click(screen.getByRole('button', { name: 'Learning' }));
      expect(assignRole).toHaveBeenCalledWith('qwen3.5-27b', 'learning');
    });

    it('waits for Settings before it can be set', () => {
      context.llmSettings = undefined;
      render(<RoleButton model={chatModel()} role={learningRole} />);
      expect(screen.getByRole('button', { name: 'Learning' })).toBeDisabled();
    });
  });
});
