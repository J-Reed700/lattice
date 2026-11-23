import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { chooseRadixOption } from './radixSelectTestUtils';
import {
  RangeStepperControl,
  SettingsNumberField,
  SettingsSelect,
} from './SettingControls';

describe('SettingControls', () => {
  it('SettingsSelect handles non-empty and empty-string options via Radix Select', async () => {
    const onChange = vi.fn();
    render(
      <SettingsSelect
        id="test-select"
        aria-label="Test select"
        value=""
        onValueChange={onChange}
        options={[
          { value: '', label: 'None' },
          { value: 'alpha', label: 'Alpha' },
          { value: 'beta', label: 'Beta' },
        ]}
      />,
    );

    const trigger = screen.getByRole('combobox', { name: 'Test select' });
    expect(trigger).toHaveTextContent('None');

    await chooseRadixOption(trigger, 'Beta');
    expect(onChange).toHaveBeenLastCalledWith('beta');
  });

  it('SettingsNumberField commits on +/- button click without blurring and clamps on blur', async () => {
    const user = userEvent.setup();
    const onCommit = vi.fn();
    render(
      <SettingsNumberField
        id="test-number"
        aria-label="Test number"
        value={5}
        min={1}
        max={10}
        step={1}
        integer
        onCommit={onCommit}
      />,
    );

    await user.click(screen.getByRole('button', { name: 'Increase value' }));
    expect(onCommit).toHaveBeenLastCalledWith(6);

    const input = screen.getByRole('spinbutton', { name: 'Test number' });
    fireEvent.change(input, { target: { value: '99' } });
    fireEvent.blur(input);
    expect(onCommit).toHaveBeenLastCalledWith(10);
  });

  it('RangeStepperControl pairs a Slider and NumberStepper with live bounds', () => {
    const onDraftChange = vi.fn();
    const onCommit = vi.fn();
    render(
      <RangeStepperControl
        id="test-range"
        label="Hybrid alpha"
        value={0.5}
        min={0}
        max={1}
        step={0.01}
        minLabel="Keyword"
        maxLabel="Vector"
        onDraftChange={onDraftChange}
        onCommit={onCommit}
      />,
    );

    expect(screen.getByText('Keyword')).toBeInTheDocument();
    expect(screen.getByText('Vector')).toBeInTheDocument();

    const slider = screen.getByLabelText('Hybrid alpha slider');
    fireEvent.change(slider, { target: { value: '0.8' } });
    expect(onDraftChange).toHaveBeenLastCalledWith(0.8);
    expect(onCommit).not.toHaveBeenCalled();

    fireEvent.pointerUp(slider);
    expect(onCommit).toHaveBeenLastCalledWith(0.8);
  });
});
