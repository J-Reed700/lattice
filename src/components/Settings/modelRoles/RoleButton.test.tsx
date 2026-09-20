import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';

import type { DownloadedModel } from '../../../types/downloadedModels';

const assignRole = vi.fn();

vi.mock('./ModelRolesContext', () => ({
  useModelRoles: () => ({ refresh: vi.fn(), assignRole }),
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
});
