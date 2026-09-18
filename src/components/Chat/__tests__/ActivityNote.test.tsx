import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ActivityNote, formatElapsed } from '../ActivityNote';

describe('ActivityNote', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('renders nothing when the turn has not said what it is doing', () => {
    const { container } = render(<ActivityNote detail={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows what the turn is doing', () => {
    const { container } = render(<ActivityNote detail="Reading thereviewgeek.com" />);
    expect(container.textContent).toBe('Reading thereviewgeek.com');
  });

  // The label alone cannot distinguish "just started" from "stuck here for six
  // minutes", which is the whole question the user was asking.
  it('adds a clock once the phase has lasted a few seconds', () => {
    const { container } = render(<ActivityNote detail="Thinking" />);
    act(() => void vi.advanceTimersByTime(5000));
    expect(container.textContent).toBe('Thinking · 5s');

    act(() => void vi.advanceTimersByTime(380_000));
    expect(container.textContent).toBe('Thinking · 6m 25s');
  });

  it('restarts the clock when the turn moves to a new phase', () => {
    const { container, rerender } = render(<ActivityNote detail="Thinking" />);
    act(() => void vi.advanceTimersByTime(90_000));
    expect(container.textContent).toBe('Thinking · 1m 30s');

    rerender(<ActivityNote detail="Searching the web" />);
    act(() => void vi.advanceTimersByTime(4000));
    expect(container.textContent).toBe('Searching the web · 4s');
  });
});

describe('formatElapsed', () => {
  it('reads as seconds under a minute and as minutes above it', () => {
    expect(formatElapsed(0)).toBe('0s');
    expect(formatElapsed(59)).toBe('59s');
    expect(formatElapsed(60)).toBe('1m 00s');
    expect(formatElapsed(383)).toBe('6m 23s');
  });
});
