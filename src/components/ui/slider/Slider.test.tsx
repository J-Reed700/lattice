import { createRef } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Slider } from './Slider';

describe('Slider', () => {
  it('renders an accessible slider with label, formatted value, and forwarded ref', () => {
    const ref = createRef<HTMLInputElement>();
    render(
      <Slider
        ref={ref}
        label="Temperature"
        min={0}
        max={2}
        step={0.1}
        value={0.7}
        showValue
        formatValue={(v) => v.toFixed(1)}
        description="Controls response creativity"
      />,
    );

    const slider = screen.getByRole('slider', { name: 'Temperature' });
    expect(ref.current).toBe(slider);
    expect(slider).toHaveAttribute('min', '0');
    expect(slider).toHaveAttribute('max', '2');
    expect(slider).toHaveAttribute('step', '0.1');
    expect(slider).toHaveValue('0.7');
    expect(slider).toHaveAttribute('aria-valuetext', '0.7');
    expect(screen.getByText('0.7')).toBeInTheDocument();
    expect(screen.getByText('Controls response creativity')).toBeInTheDocument();
  });

  it('supports uncontrolled updates and calls onValueChange', () => {
    const onValueChange = vi.fn();
    render(<Slider label="Volume" min={0} max={100} defaultValue={25} showValue onValueChange={onValueChange} />);
    const slider = screen.getByRole('slider', { name: 'Volume' });
    expect(screen.getByText('25')).toBeInTheDocument();

    fireEvent.change(slider, { target: { value: '60' } });
    expect(onValueChange).toHaveBeenCalledWith(60);
    expect(screen.getByText('60')).toBeInTheDocument();
  });

  it('handles disabled state and min/max edge captions', () => {
    render(<Slider label="Depth" min={1} max={5} value={3} minLabel="Brief" maxLabel="Thorough" disabled />);
    const slider = screen.getByRole('slider', { name: 'Depth' });
    expect(slider).toBeDisabled();
    expect(screen.getByText('Brief')).toBeInTheDocument();
    expect(screen.getByText('Thorough')).toBeInTheDocument();
  });

  it('guards against degenerate min === max range without NaN widths', () => {
    const { container } = render(<Slider label="Locked" min={5} max={5} value={5} />);
    const fill = container.querySelector('.bg-accent') as HTMLElement | null;
    expect(fill?.style.width).toBe('0%');
  });
});
