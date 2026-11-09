import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { StatusLine } from './StatusLine';

describe('StatusLine', () => {
  it('renders nothing when idle', () => {
    const { container } = render(<StatusLine state={{ kind: 'idle' }} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('announces a busy label politely as a status', () => {
    render(<StatusLine state={{ kind: 'busy', label: 'Reading your code…' }} />);
    const status = screen.getByRole('status');
    expect(status).toHaveTextContent('Reading your code…');
    expect(status).toHaveAttribute('aria-live', 'polite');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows an error as an alert and calls retry from Try again', () => {
    const retry = vi.fn();
    render(<StatusLine state={{ kind: 'error', message: 'The reply failed.', retry }} />);
    expect(screen.getByRole('alert')).toHaveTextContent('The reply failed.');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it('has no Try again button without a retry', () => {
    render(<StatusLine state={{ kind: 'error', message: 'Nothing to redo.' }} />);
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument();
  });
});
