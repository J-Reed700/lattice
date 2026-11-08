import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import {
  PracticeEmptyState,
  PracticeLoadingState,
  StatusLine,
} from './StatusLine';

describe('StatusLine', () => {
  it('renders nothing when there is neither busy state nor error', () => {
    const { container } = render(<StatusLine />);
    expect(container).toBeEmptyDOMElement();
  });

  it('announces busy state via role="status"', () => {
    render(<StatusLine busy="Evaluating your solution…" />);
    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('Evaluating your solution…');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('announces error state via role="alert"', () => {
    render(<StatusLine error="The connection timed out." />);
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('The connection timed out.');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('calls onRetry when "Try again" is clicked', () => {
    const onRetry = vi.fn();
    render(<StatusLine error="Could not save changes." onRetry={onRetry} />);

    const retryButton = screen.getByRole('button', { name: /try again/i });
    expect(retryButton).toBeInTheDocument();
    fireEvent.click(retryButton);
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('omits the retry button when onRetry is not provided', () => {
    render(<StatusLine error="Read-only error." />);
    expect(screen.getByRole('alert')).toHaveTextContent('Read-only error.');
    expect(screen.queryByRole('button', { name: /try again/i })).not.toBeInTheDocument();
  });
});

describe('PracticeLoadingState', () => {
  it('announces loading status via role="status"', () => {
    render(<PracticeLoadingState label="Preparing practice session…" />);
    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('Preparing practice session…');
  });
});

describe('PracticeEmptyState', () => {
  it('renders title, description, and action button', () => {
    const onAction = vi.fn();
    render(
      <PracticeEmptyState
        title="No practice attempts yet"
        description="Start your first attempt to build skill."
        action={<button type="button" onClick={onAction}>Start session</button>}
      />,
    );

    expect(screen.getByText('No practice attempts yet')).toBeInTheDocument();
    expect(screen.getByText('Start your first attempt to build skill.')).toBeInTheDocument();
    const actionButton = screen.getByRole('button', { name: 'Start session' });
    fireEvent.click(actionButton);
    expect(onAction).toHaveBeenCalledTimes(1);
  });
});
