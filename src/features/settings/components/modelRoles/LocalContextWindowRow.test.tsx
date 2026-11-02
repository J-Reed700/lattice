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
const modelMax = () => screen.getByRole('checkbox', { name: "Model's max" });
const windowField = () => screen.getByLabelText('Default context window');

describe('LocalContextWindowRow', () => {
  beforeEach(() => {
    mockUpdateSettings.mockReset().mockResolvedValue({});
  });

  it('unticking "Model\'s max" saves nothing and leaves the field empty for the user', async () => {
    renderRow();

    await userEvent.setup().click(modelMax());

    expect(mockUpdateSettings).not.toHaveBeenCalled();
    expect(modelMax()).not.toBeChecked();
    expect(windowField()).toBeEnabled();
    expect(windowField()).toHaveValue(null);
  });

  it('saves the window the user types, within the bounds', async () => {
    renderRow();

    await userEvent.setup().click(modelMax());
    fireEvent.change(windowField(), { target: { value: '60416' } });
    fireEvent.blur(windowField());
    fireEvent.change(windowField(), { target: { value: '100' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 60_416 }, { localContextWindow: 512 }]);
    expect(modelMax()).not.toBeChecked();
    expect(windowField()).toHaveValue(512);
  });

  it('reaches the largest window a model here is trained for', async () => {
    renderRow(null, trainedFor([32_768, 1_048_576, null]));

    await userEvent.setup().click(modelMax());
    fireEvent.change(windowField(), { target: { value: '1048576' } });
    fireEvent.blur(windowField());
    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 1_048_576 }]);
    expect(windowField()).toHaveAttribute('max', '1048576');
  });

  it('keeps a ceiling when no model reports its window', () => {
    renderRow(32_768, trainedFor([null]));

    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 262_144 }]);
  });

  it('keeps a ceiling when the list of models is empty', () => {
    renderRow(32_768, []);

    expect(windowField()).toHaveAttribute('max', '262144');

    fireEvent.change(windowField(), { target: { value: '9000000' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: 262_144 }]);
    expect(windowField()).toHaveValue(262_144);
  });

  it('leaving the field empty keeps every model at its maximum', async () => {
    renderRow();

    await userEvent.setup().click(modelMax());
    fireEvent.blur(windowField());

    expect(mockUpdateSettings).not.toHaveBeenCalled();
    expect(modelMax()).toBeChecked();
    expect(windowField()).toBeDisabled();
  });

  it("clearing a saved window goes back to each model's maximum", () => {
    renderRow(32_768);

    fireEvent.change(windowField(), { target: { value: '' } });
    fireEvent.blur(windowField());

    expect(saved()).toEqual([{ localContextWindow: null }]);
    expect(modelMax()).toBeChecked();
  });

  it('shows the saved window again when saving fails', async () => {
    mockUpdateSettings.mockRejectedValue(new Error('disk full'));
    renderRow(32_768);

    fireEvent.change(windowField(), { target: { value: '16384' } });
    fireEvent.blur(windowField());

    expect(await screen.findByDisplayValue('32768')).toBe(windowField());
  });
});
