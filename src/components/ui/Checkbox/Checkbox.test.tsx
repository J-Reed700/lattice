import { createRef } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import Checkbox from './Checkbox';

describe('Checkbox', () => {
  it('uses a stable React useId across rerenders and associates label and description', () => {
    const ref = createRef<HTMLInputElement>();
    const { rerender } = render(
      <Checkbox ref={ref} label="Enable offline mode" description="Keep embeddings local" />,
    );
    const input = screen.getByRole('checkbox', { name: 'Enable offline mode' });
    expect(ref.current).toBe(input);
    const firstId = input.id;
    expect(firstId).toMatch(/^checkbox-/);
    expect(input).toHaveAttribute('aria-describedby', `${firstId}-description`);

    rerender(<Checkbox ref={ref} label="Enable offline mode" description="Updated text" />);
    expect(screen.getByRole('checkbox', { name: 'Enable offline mode' }).id).toBe(firstId);
  });

  it('toggles in uncontrolled mode and notifies onCheckedChange', () => {
    const onCheckedChange = vi.fn();
    render(<Checkbox label="Auto-save" defaultChecked={false} onCheckedChange={onCheckedChange} />);
    const input = screen.getByRole('checkbox', { name: 'Auto-save' });
    expect(input).not.toBeChecked();

    fireEvent.click(input);
    expect(input).toBeChecked();
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it('syncs DOM indeterminate state and aria-checked="mixed"', () => {
    render(<Checkbox label="Select all" checked={false} indeterminate onCheckedChange={() => {}} />);
    const input = screen.getByRole('checkbox', { name: 'Select all' }) as HTMLInputElement;
    expect(input.indeterminate).toBe(true);
    expect(input).toHaveAttribute('aria-checked', 'mixed');
  });

  it('renders error state with alert role and aria-invalid', () => {
    render(<Checkbox id="tos" label="Accept terms" error="Required to continue" />);
    const input = screen.getByRole('checkbox', { name: 'Accept terms' });
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'tos-error');
    expect(screen.getByRole('alert')).toHaveTextContent('Required to continue');
  });

  it('wires both description and error ids into aria-describedby when both are present', () => {
    render(
      <Checkbox
        id="consent"
        label="Share diagnostics"
        description="Anonymous crash traces only"
        error="Selection required"
      />,
    );
    const input = screen.getByRole('checkbox', { name: 'Share diagnostics' });
    expect(input).toHaveAttribute('aria-describedby', 'consent-description consent-error');
    expect(screen.getByText('Anonymous crash traces only')).toHaveAttribute('id', 'consent-description');
    expect(screen.getByRole('alert')).toHaveAttribute('id', 'consent-error');
  });
});
