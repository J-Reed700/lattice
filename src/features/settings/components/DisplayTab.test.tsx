import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';

import { DisplayTab } from './DisplayTab';

const h = vi.hoisted(() => ({
  settings: {} as Record<string, any>,
  update: { mutate: vi.fn(), isPending: false, isError: false },
  version: undefined as undefined | { version: string; commitHash?: string },
  check: {} as Record<string, any>,
  commands: [] as PaletteCommand[],
}));

vi.mock('../hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => h.settings,
  useUpdateSettingsMutation: () => h.update,
}));
vi.mock('../hooks/useUpdateCheckQuery', () => ({
  useVersionInfoQuery: () => ({ data: h.version }),
  useCheckForUpdatesMutation: () => h.check,
}));
vi.mock('@/features/palette/hooks/useRegisterPaletteCommands', () => ({
  useRegisterPaletteCommands: (commands: PaletteCommand[]) => { h.commands = commands; },
}));

describe('DisplayTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = { data: { ui: { theme: 'dark' } }, isLoading: false, isError: false, refetch: vi.fn() };
    h.update = { mutate: vi.fn(), isPending: false, isError: false };
    h.version = { version: '0.9.1', commitHash: 'abc1234' };
    h.check = { mutate: vi.fn(), isPending: false, isError: false, data: undefined };
  });

  it('marks the saved theme and saves a new one', () => {
    render(<DisplayTab />);
    expect(screen.getByRole('radio', { name: 'Dark' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: 'Dark' })).toHaveAttribute('tabindex', '0');
    expect(screen.getByRole('radio', { name: 'Light' })).toHaveAttribute('tabindex', '-1');
    fireEvent.click(screen.getByRole('radio', { name: 'Light' }));
    expect(h.update.mutate).toHaveBeenCalledWith({ category: 'ui', updates: { theme: 'light' } });
  });

  it('follows the system before settings load, with the choices disabled', () => {
    h.settings = { data: undefined, isLoading: true, isError: false, refetch: vi.fn() };
    render(<DisplayTab />);
    expect(screen.getByRole('radio', { name: 'System' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: 'Light' })).toBeDisabled();
  });

  it('moves through themes with the arrow keys, wrapping round', () => {
    render(<DisplayTab />);
    fireEvent.keyDown(screen.getByRole('radio', { name: 'Dark' }), { key: 'ArrowRight' });
    expect(h.update.mutate).toHaveBeenLastCalledWith({ category: 'ui', updates: { theme: 'system' } });
    expect(screen.getByRole('radio', { name: 'System' })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('radio', { name: 'Light' }), { key: 'ArrowUp' });
    expect(h.update.mutate).toHaveBeenLastCalledWith({ category: 'ui', updates: { theme: 'system' } });
    fireEvent.keyDown(screen.getByRole('radio', { name: 'System' }), { key: 'ArrowDown' });
    expect(h.update.mutate).toHaveBeenLastCalledWith({ category: 'ui', updates: { theme: 'light' } });
    fireEvent.keyDown(screen.getByRole('radio', { name: 'Dark' }), { key: 'ArrowLeft' });
    expect(h.update.mutate).toHaveBeenLastCalledWith({ category: 'ui', updates: { theme: 'light' } });
    const calls = h.update.mutate.mock.calls.length;
    fireEvent.keyDown(screen.getByRole('radio', { name: 'Dark' }), { key: 'Enter' });
    expect(h.update.mutate).toHaveBeenCalledTimes(calls);
  });

  it('offers a retry when settings fail to load, and reports a failed save', () => {
    h.settings = { ...h.settings, isError: true };
    h.update.isError = true;
    render(<DisplayTab />);
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(h.settings.refetch).toHaveBeenCalled();
    expect(screen.getByText("Couldn't save your theme. Try selecting it again.")).toBeInTheDocument();
  });

  it('shows the version and build', () => {
    render(<DisplayTab />);
    expect(screen.getByText('0.9.1')).toBeInTheDocument();
    expect(screen.getByText('Build abc1234')).toBeInTheDocument();
  });

  it('shows a dash before the version is known', () => {
    h.version = undefined;
    render(<DisplayTab />);
    expect(screen.getByText('—')).toBeInTheDocument();
  });

  describe('updates', () => {
    it('checks from the button and the palette', () => {
      render(<DisplayTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }));
      const command = h.commands.find((entry) => entry.id === 'app.checkUpdates')!;
      expect(command.enabled).toBe(true);
      command.run();
      expect(h.check.mutate).toHaveBeenCalledTimes(2);
    });

    it('shows a check in progress', () => {
      h.check.isPending = true;
      render(<DisplayTab />);
      expect(screen.getByRole('button', { name: 'Checking…' })).toBeDisabled();
      expect(screen.getAllByText('Checking…')).toHaveLength(2);
      expect(h.commands[0].enabled).toBe(false);
    });

    it('offers the download for a newer version', () => {
      h.check.data = { available: true, latestVersion: '1.0.0', downloadUrl: 'https://example.org/lattice.dmg' };
      render(<DisplayTab />);
      expect(screen.getByText('Version 1.0.0 is available.')).toBeInTheDocument();
      expect(screen.getByRole('link', { name: 'Download' })).toHaveAttribute('href', 'https://example.org/lattice.dmg');
    });

    it('says up to date, and when the check failed', () => {
      h.check.data = { available: false };
      const first = render(<DisplayTab />);
      expect(screen.getByText('Up to date.')).toBeInTheDocument();
      expect(screen.queryByRole('link', { name: 'Download' })).not.toBeInTheDocument();
      first.unmount();
      h.check = { ...h.check, isError: true };
      render(<DisplayTab />);
      expect(screen.getByText("Couldn't check for updates. Try again in a minute.")).toBeInTheDocument();
    });
  });
});
