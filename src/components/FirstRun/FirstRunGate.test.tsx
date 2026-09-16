import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FirstRunGate } from './FirstRunGate';
import { VaultAPI } from '../../lib/api';
import { makeAppSettings } from '../../tests/fixtures/appSettings';

vi.mock('../../routes', () => ({
  router: { navigate: vi.fn() },
}));

vi.mock('../../hooks/useModelCatalog', () => ({
  useModelCatalog: () => ({ systemCapabilities: null }),
}));

const FIRST_RUN_STATUS = JSON.stringify({
  needs_setup: true,
  recommended_model_id: null,
  recommended_model_name: null,
  estimated_size_bytes: null,
  embedding_model: {
    model_id: 'bge-small',
    display_name: 'BGE Small',
    estimated_size_bytes: 134_217_728,
  },
  chat_model: {
    model_id: 'qwen3-4b',
    display_name: 'Qwen3 4B',
    estimated_size_bytes: 2_684_354_560,
  },
  total_estimated_size_bytes: 2_818_572_288,
});

function settingsWith(firstRunDismissed: boolean) {
  return makeAppSettings({ onboarding: { firstRunDismissed } });
}

function renderGate() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<FirstRunGate />, { wrapper });
}

describe('FirstRunGate', () => {
  // A settings document that actually remembers a write, so the refetch the
  // mutation triggers answers with what was stored rather than the old value.
  let storedDismissal = false;

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    storedDismissal = false;
    vi.mocked(invoke).mockResolvedValue(FIRST_RUN_STATUS);
    vi.spyOn(VaultAPI, 'getSettings').mockImplementation(async () => ({
      ok: true,
      data: settingsWith(storedDismissal),
    }));
    vi.spyOn(VaultAPI, 'updateSettings').mockImplementation(async () => {
      storedDismissal = true;
      return { ok: true, data: settingsWith(true) };
    });
  });

  it('asks the backend and offers the bundle when nothing has been dismissed', async () => {
    renderGate();

    expect(await screen.findByText('Set up AI')).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith('plugin:model|check_first_run_status');
  });

  it('stays quiet when the setting says the user already dismissed it', async () => {
    storedDismissal = true;

    renderGate();

    await waitFor(() => expect(VaultAPI.getSettings).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.queryByText('Set up AI')).not.toBeInTheDocument();
  });

  it('records "Not now" against the repository', async () => {
    const user = userEvent.setup();
    renderGate();

    await user.click(await screen.findByRole('button', { name: 'Not now' }));

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'onboarding',
        updates: { firstRunDismissed: true },
      });
    });
  });
});
