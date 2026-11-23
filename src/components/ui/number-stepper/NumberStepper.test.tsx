import { createRef } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { NumberStepper } from './NumberStepper';

describe('NumberStepper', () => {
  it('renders a spinbutton with label, suffix, and forwarded ref', () => {
    const ref = createRef<HTMLInputElement>();
    render(
      <NumberStepper
        ref={ref}
        label="Timeout"
        value={30}
        min={5}
        max={120}
        step={5}
        suffix="s"
        helperText="Seconds before canceling"
      />,
    );

    const input = screen.getByRole('spinbutton', { name: 'Timeout' });
    expect(ref.current).toBe(input);
    expect(input).toHaveValue('30');
    expect(input).toHaveAttribute('aria-valuemin', '5');
    expect(input).toHaveAttribute('aria-valuemax', '120');
    expect(input).toHaveAttribute('aria-valuenow', '30');
    expect(screen.getByText('s')).toBeInTheDocument();
    expect(screen.getByText('Seconds before canceling')).toBeInTheDocument();
  });

  it('increments and decrements with buttons and respects floating-point precision', () => {
    const onValueChange = vi.fn();
    render(
      <NumberStepper
        label="Temperature"
        defaultValue={0.1}
        min={0}
        max={1}
        step={0.2}
        onValueChange={onValueChange}
      />,
    );

    const inc = screen.getByRole('button', { name: 'Increase value' });
    const dec = screen.getByRole('button', { name: 'Decrease value' });

    fireEvent.click(inc);
    expect(onValueChange).toHaveBeenLastCalledWith(0.3);
    expect(screen.getByRole('spinbutton')).toHaveValue('0.3');

    fireEvent.click(dec);
    expect(onValueChange).toHaveBeenLastCalledWith(0.1);
  });

  it('clamps typed out-of-range values on blur and reverts invalid text', () => {
    const onValueChange = vi.fn();
    render(<NumberStepper label="Limit" defaultValue={10} min={1} max={50} onValueChange={onValueChange} />);
    const input = screen.getByRole('spinbutton', { name: 'Limit' });

    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '999' } });
    fireEvent.blur(input);
    expect(onValueChange).toHaveBeenLastCalledWith(50);
    expect(input).toHaveValue('50');

    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: 'abc' } });
    fireEvent.blur(input);
    expect(input).toHaveValue('50');
  });

  it('supports keyboard stepping with ArrowUp, ArrowDown, PageUp, PageDown, Home, and End', () => {
    const onValueChange = vi.fn();
    render(<NumberStepper label="Retries" defaultValue={5} min={0} max={20} step={1} onValueChange={onValueChange} />);
    const input = screen.getByRole('spinbutton', { name: 'Retries' });

    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(input).toHaveValue('6');

    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input).toHaveValue('5');

    fireEvent.keyDown(input, { key: 'PageUp' });
    expect(input).toHaveValue('15');

    fireEvent.keyDown(input, { key: 'End' });
    expect(input).toHaveValue('20');

    fireEvent.keyDown(input, { key: 'Home' });
    expect(input).toHaveValue('0');

    fireEvent.keyDown(input, { key: 'PageDown' });
    expect(input).toHaveValue('0');
  });

  it('disables step buttons at min/max boundaries and renders error state', () => {
    render(<NumberStepper label="Count" value={0} min={0} max={0} error="No capacity" />);
    expect(screen.getByRole('button', { name: 'Decrease value' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Increase value' })).toBeDisabled();
    expect(screen.getByRole('spinbutton')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('alert')).toHaveTextContent('No capacity');
  });

  it('steps from clamped uncommitted draft without blur-click race when clicking decrement', () => {
    const onValueChange = vi.fn();
    render(<NumberStepper label="Percent" defaultValue={50} min={0} max={100} step={1} onValueChange={onValueChange} />);
    const input = screen.getByRole('spinbutton', { name: 'Percent' });
    const dec = screen.getByRole('button', { name: 'Decrease value' });

    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '120' } });
    fireEvent.mouseDown(dec);
    fireEvent.click(dec);

    expect(onValueChange).toHaveBeenLastCalledWith(99);
    expect(input).toHaveValue('99');
  });

  it('stays synced with controlled value if parent rejects or normalizes onValueChange', () => {
    const onValueChange = vi.fn();
    render(<NumberStepper label="Fixed" value={10} min={0} max={100} step={1} onValueChange={onValueChange} />);
    const input = screen.getByRole('spinbutton', { name: 'Fixed' });
    const inc = screen.getByRole('button', { name: 'Increase value' });

    fireEvent.focus(input);
    fireEvent.click(inc);
    expect(onValueChange).toHaveBeenCalledWith(11);
    expect(input).toHaveValue('10');
    expect(input).toHaveAttribute('aria-valuenow', '10');

    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(input).toHaveValue('10');
  });
});
