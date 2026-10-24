import { open } from '@tauri-apps/plugin-dialog';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultTab } from './VaultTab';

type Callbacks = { onError?: (error: Error) => void; onSuccess?: () => void };

const h = vi.hoisted(() => ({
  settings: { data: undefined as unknown, isLoading: false },
  mutation: { mutate: vi.fn(), isPending: false },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('../hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => h.settings,
  useUpdateSettingsMutation: () => h.mutation,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('./BackupSection', () => ({ BackupSection: () => <section aria-label="Backups" /> }));

const lastCallbacks = (): Callbacks => h.mutation.mutate.mock.calls.at(-1)![1];

describe('VaultTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = { data: { vault: { vaultPath: '/Users/ada/Garden', enabled: true, watchExternalChanges: false } }, isLoading: false };
    h.mutation.isPending = false;
  });

  it('shows the folder and sync switches', () => {
    render(<VaultTab />);
    expect(screen.getByText('/Users/ada/Garden')).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Mirror notes to disk' })).toBeChecked();
    expect(screen.getByRole('switch', { name: 'Watch for external edits' })).not.toBeChecked();
    expect(screen.getByRole('region', { name: 'Backups' })).toBeInTheDocument();
  });

  it('falls back to the default folder and hides Reset', () => {
    h.settings = { data: undefined, isLoading: false };
    render(<VaultTab />);
    expect(screen.getByText('~/Lattice')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Mirror notes to disk' })).not.toBeChecked();
  });

  it('disables controls while settings load or save', () => {
    h.mutation.isPending = true;
    render(<VaultTab />);
    expect(screen.getByRole('button', { name: 'Choose' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Reset' })).toBeDisabled();
    expect(screen.getByRole('switch', { name: 'Mirror notes to disk' })).toBeDisabled();
  });

  describe('choosing a folder', () => {
    it('saves the picked folder and reports either way', async () => {
      vi.mocked(open).mockResolvedValueOnce('/Volumes/Notes');
      render(<VaultTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Choose' }));
      await waitFor(() => expect(h.mutation.mutate).toHaveBeenCalled());
      expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, title: 'Select Vault Folder' });
      expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'vault', updates: { vaultPath: '/Volumes/Notes' } });
      lastCallbacks().onSuccess?.();
      expect(h.toast.success).toHaveBeenCalledWith('Vault folder updated');
      lastCallbacks().onError?.(new Error('read-only volume'));
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't save vault folder", { message: 'read-only volume' });
      await waitFor(() => expect(screen.getByRole('button', { name: 'Choose' })).toBeEnabled());
    });

    it('ignores a cancelled or blank pick', async () => {
      vi.mocked(open).mockResolvedValueOnce(null);
      render(<VaultTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Choose' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Choose' })).toBeEnabled());
      vi.mocked(open).mockResolvedValueOnce('   ');
      fireEvent.click(screen.getByRole('button', { name: 'Choose' }));
      await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
      expect(h.mutation.mutate).not.toHaveBeenCalled();
    });
  });

  it('resets the folder to the default', () => {
    render(<VaultTab />);
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
    expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'vault', updates: { vaultPath: '' } });
    lastCallbacks().onSuccess?.();
    expect(h.toast.success).toHaveBeenCalledWith('Vault folder reset to default (~/Lattice)');
    lastCallbacks().onError?.(new Error('locked'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't reset vault folder", { message: 'locked' });
  });

  it('turns mirroring off and on', () => {
    render(<VaultTab />);
    fireEvent.click(screen.getByRole('switch', { name: 'Mirror notes to disk' }));
    expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'vault', updates: { enabled: false } });
    lastCallbacks().onSuccess?.();
    expect(h.toast.success).toHaveBeenCalledWith('Vault export disabled');
    lastCallbacks().onError?.(new Error('nope'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't toggle vault export", { message: 'nope' });
  });

  it('says mirroring backfills when turned on', () => {
    h.settings = { data: { vault: { vaultPath: '', enabled: false, watchExternalChanges: true } }, isLoading: false };
    render(<VaultTab />);
    fireEvent.click(screen.getByRole('switch', { name: 'Mirror notes to disk' }));
    lastCallbacks().onSuccess?.();
    expect(h.toast.success).toHaveBeenCalledWith('Vault export enabled — backfilling existing notes');
  });

  it('toggles watching for external edits', () => {
    render(<VaultTab />);
    fireEvent.click(screen.getByRole('switch', { name: 'Watch for external edits' }));
    expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'vault', updates: { watchExternalChanges: true } });
    lastCallbacks().onError?.(new Error('watcher limit'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't toggle external watcher", { message: 'watcher limit' });
  });
});
