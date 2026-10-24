import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { DatePicker } from './DatePicker';

const NOW = new Date(2026, 9, 24, 9, 30);
const trigger = () => screen.getByRole('button', { name: /Choose date/ });
const calendar = () => screen.queryByRole('dialog', { name: 'Date picker calendar' });
const day = (label: RegExp) => screen.getByRole('button', { name: label });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe('DatePicker', () => {
  it('shows the placeholder and picks a day', () => {
    const onSelect = vi.fn();
    render(<DatePicker onSelect={onSelect} placeholder="Harvest date" />);
    expect(trigger()).toHaveTextContent('Harvest date');
    expect(trigger()).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(trigger());
    expect(trigger()).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(day(/October 20th, 2026/));
    expect(onSelect).toHaveBeenCalledWith(new Date(2026, 9, 20));
    expect(calendar()).not.toBeInTheDocument();
  });

  it('shows the chosen date in the given format and clears it without opening', () => {
    const onSelect = vi.fn();
    const { rerender } = render(<DatePicker onSelect={onSelect} selected={new Date(2026, 9, 21)} />);
    expect(trigger()).toHaveTextContent('Oct 21, 2026');
    rerender(<DatePicker onSelect={onSelect} selected={new Date(2026, 9, 21)} dateFormat="yyyy-MM-dd" />);
    expect(trigger()).toHaveTextContent('2026-10-21');
    fireEvent.click(screen.getByRole('button', { name: 'Clear date' }));
    expect(onSelect).toHaveBeenCalledWith(undefined);
    expect(calendar()).not.toBeInTheDocument();
  });

  it('cannot be opened or cleared while disabled', () => {
    render(<DatePicker onSelect={vi.fn()} selected={new Date(2026, 9, 21)} disabled />);
    expect(trigger()).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Clear date' })).not.toBeInTheDocument();
    fireEvent.click(trigger());
    expect(calendar()).not.toBeInTheDocument();
  });

  it('picks today from the quick action', () => {
    const onSelect = vi.fn();
    render(<DatePicker onSelect={onSelect} />);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole('button', { name: 'Today' }));
    expect(onSelect).toHaveBeenCalledWith(NOW);
    expect(calendar()).not.toBeInTheDocument();
  });

  it('closes from its button, Escape, the trigger or a click outside, but not a click inside', () => {
    render(<div><DatePicker onSelect={vi.fn()} /><p>Outside</p></div>);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(calendar()).not.toBeInTheDocument();

    fireEvent.click(trigger());
    fireEvent.keyDown(document, { key: 'Enter' });
    expect(calendar()).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(calendar()).not.toBeInTheDocument();

    fireEvent.click(trigger());
    fireEvent.mouseDown(calendar()!);
    expect(calendar()).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByText('Outside'));
    expect(calendar()).not.toBeInTheDocument();

    fireEvent.click(trigger());
    fireEvent.click(trigger());
    expect(calendar()).not.toBeInTheDocument();
  });

  it('disables days outside the allowed range', () => {
    render(<DatePicker onSelect={vi.fn()} minDate={new Date(2026, 9, 10)} maxDate={new Date(2026, 9, 25)} />);
    fireEvent.click(trigger());
    expect(day(/October 5th, 2026/)).toBeDisabled();
    expect(day(/October 28th, 2026/)).toBeDisabled();
    expect(day(/October 15th, 2026/)).toBeEnabled();
  });
});
