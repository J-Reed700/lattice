import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PrivacyTab } from './PrivacyTab';

const h = vi.hoisted(() => ({
  settings: { data: undefined as unknown, isLoading: false },
  mutation: { mutate: vi.fn(), isPending: false },
  toast: { error: vi.fn() },
}));

vi.mock('../hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => h.settings,
  useUpdateSettingsMutation: () => h.mutation,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

describe('PrivacyTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = { data: { privacy: { telemetryEnabled: true, crashReporting: false } }, isLoading: false };
    h.mutation.isPending = false;
  });

  it('shows the saved choices and saves a change', () => {
    render(<PrivacyTab />);
    expect(screen.getByRole('switch', { name: 'Anonymous usage statistics' })).toBeChecked();
    expect(screen.getByRole('switch', { name: 'Crash reports' })).not.toBeChecked();
    fireEvent.click(screen.getByRole('switch', { name: 'Anonymous usage statistics' }));
    expect(h.mutation.mutate).toHaveBeenCalledWith({ category: 'privacy', updates: { telemetryEnabled: false } }, expect.anything());
    fireEvent.click(screen.getByRole('switch', { name: 'Crash reports' }));
    expect(h.mutation.mutate).toHaveBeenLastCalledWith({ category: 'privacy', updates: { crashReporting: true } }, expect.anything());
  });

  it('reports a setting that could not be saved', () => {
    render(<PrivacyTab />);
    fireEvent.click(screen.getByRole('switch', { name: 'Crash reports' }));
    h.mutation.mutate.mock.calls[0][1].onError(new Error('settings file locked'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't save privacy setting", { message: 'settings file locked' });
  });

  it('defaults to sharing nothing and waits while loading or saving', () => {
    h.settings = { data: undefined, isLoading: true };
    render(<PrivacyTab />);
    expect(screen.getByRole('switch', { name: 'Anonymous usage statistics' })).not.toBeChecked();
    expect(screen.getByRole('switch', { name: 'Anonymous usage statistics' })).toBeDisabled();
    expect(screen.getByRole('switch', { name: 'Crash reports' })).toBeDisabled();
  });
});
