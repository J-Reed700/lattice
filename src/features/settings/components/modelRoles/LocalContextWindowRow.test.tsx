import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { makeAppSettings } from '@/tests/fixtures/appSettings';
import type { DownloadedModel } from '@/types/downloadedModels';

import { LocalContextWindowRow } from './LocalContextWindowRow';

const mockUpdateSettings = vi.fn();
let mockSettings = makeAppSettings();

vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: mockSettings }),
  useUpdateSettingsMutation: () => ({
    mutateAsync: (...args: unknown[]) => mockUpdateSettings(...args),
    isPending: false,
  }),
}));

const trainedFor = (windows: Array<number | null>) =>
  windows.map((trained_context_length) => ({ trained_context_length }) as DownloadedModel);

function renderRow(localContextWindow: number | null = null, models: DownloadedModel[] = []) {
  mockSettings = makeAppSettings();
  mockSettings.llm.localContextWindow = localContextWindow;
  render(<LocalContextWindowRow models={models} />);
}

const saved = () => mockUpdateSettings.mock.calls.map(([update]) => update.updates);
const fitEach = () => screen.getByRole('checkbox', { name: 'Fit each model to this machine' });
const windowField = () => screen.getByLabelText('Default context window');

describe('LocalContextWindowRow', () => {
  beforeEach(() => {
    mockUpdateSettings.mockReset().mockResolvedValue({});
  });

  it('unticking "Fit each model to this machine" saves nothing and leaves the field empty for the user', async () => {
    renderRow();

    await userEvent.setup().click(fitEach());

    expect(mockUpdateSettings).not.toHaveBeenCalled();
    expect(fitEach()).not.toBeChecked();
    expect(windowField()).toBeEnabled();
    expect(windowField()).toHaveValue(null);
  });

  it('saves the window the user types, at least one token', async () => {
    renderRow();

    await userEvent.setup().click(fitEach());
    fireEvent.change(windowField(), { target: { value: '60416' } });
    fireEvent.blur(windowField());
    fireEvent.change(windowField(), { target: { value: '100' } });
    fireEvent.blur(windowField());
    fireEvent.change(windowField(), { target: { value: '0' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 60_416 }, { localContextWindow: 100 }, { localContextWindow: 1 }]);
    expect(fitEach()).not.toBeChecked();
    expect(windowField()).toHaveValue(1);
  });

  it('reaches the largest window a model here is trained for', async () => {
    renderRow(null, trainedFor([32_768, 1_048_576, null]));

    await userEvent.setup().click(fitEach());
    fireEvent.change(windowField(), { target: { value: '1048576' } });
    fireEvent.blur(windowField());
    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 1_048_576 }]);
    expect(windowField()).toHaveAttribute('max', '1048576');
  });

  it('sets no ceiling when no model reports its window', () => {
    renderRow(32_768, trainedFor([null]));

    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 9_000_000 }]);
  });

  it('sets no ceiling when the list of models is empty', () => {
    renderRow(32_768, []);

    expect(windowField()).not.toHaveAttribute('max');

    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 9_000_000 }]);
    expect(windowField()).toHaveValue(9_000_000);
  });

  it('leaving the field empty keeps no default window, so each model fits this machine', async () => {
    renderRow();

    await userEvent.setup().click(fitEach());
    fireEvent.blur(windowField());

    expect(mockUpdateSettings).not.toHaveBeenCalled();
    expect(fitEach()).toBeChecked();
    expect(windowField()).toBeDisabled();
  });

  it('clearing a saved window goes back to fitting each model to this machine', () => {
    renderRow(32_768);

    fireEvent.change(windowField(), { target: { value: '' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: null }]);
    expect(fitEach()).toBeChecked();
  });

  it('shows the saved window again when saving fails', async () => {
    mockUpdateSettings.mockRejectedValue(new Error('disk full'));
    renderRow(32_768);

    fireEvent.change(windowField(), { target: { value: '16384' } });
    fireEvent.blur(windowField());

    expect(await screen.findByDisplayValue('32768')).toBe(windowField());
  });
});
