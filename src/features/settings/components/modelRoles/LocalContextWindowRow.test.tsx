import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { makeAppSettings } from '@/tests/fixtures/appSettings';

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

function renderRow(localContextWindow: number | null = null) {
  mockSettings = makeAppSettings();
  mockSettings.llm.localContextWindow = localContextWindow;
  render(<LocalContextWindowRow />);
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
