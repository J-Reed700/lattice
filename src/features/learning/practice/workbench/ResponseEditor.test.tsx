import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ResponseEditor, type ResponseEditorProps } from './ResponseEditor';

function setup(overrides: Partial<ResponseEditorProps> = {}) {
  const props: ResponseEditorProps = {
    text: 'My answer', dirty: false, active: true, autosave: { state: 'idle', error: null, flush: vi.fn(async () => {}) },
    actionError: '', onRetryAction: undefined, conflict: false, busy: false, submitting: false,
    onChange: vi.fn(), onReload: vi.fn(), onSubmit: vi.fn(), ...overrides,
  };
  render(<ResponseEditor {...props} />);
  return props;
}

describe('ResponseEditor', () => {
  it('shows Saved when nothing is waiting', () => {
    setup();
    expect(screen.getByRole('status')).toHaveTextContent('Saved');
  });

  it('shows Unsaved changes when something is waiting', () => {
    setup({ dirty: true });
    expect(screen.getByRole('status')).toHaveTextContent('Unsaved changes');
  });

  it('shows Saving… while a save is in flight', () => {
    setup({ dirty: true, autosave: { state: 'saving', error: null, flush: vi.fn() } });
    expect(screen.getByRole('status')).toHaveTextContent('Saving…');
  });

  it('shows a failed save as unsaved, with the error as an alert and Try again, which flushes', async () => {
    const flush = vi.fn(async () => {});
    setup({ dirty: true, autosave: { state: 'error', error: 'Connection interrupted', flush } });
    expect(screen.getByRole('status')).toHaveTextContent('Unsaved changes');
    expect(screen.getByRole('alert')).toHaveTextContent('Connection interrupted');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Try again' }));
    expect(flush).toHaveBeenCalledTimes(1);
  });

  it('offers to reload the saved attempt and keep the text after a conflict', async () => {
    const props = setup({ dirty: true, conflict: true, autosave: { state: 'conflict', error: 'Stale', flush: vi.fn() } });
    expect(screen.getByRole('status')).toHaveTextContent('Unsaved changes');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Reload the saved attempt · keep my text' }));
    expect(props.onReload).toHaveBeenCalledTimes(1);
  });

  it('retries a failed action from its own alert', async () => {
    const onRetryAction = vi.fn();
    setup({ actionError: 'The tutor could not answer.', onRetryAction });
    await userEvent.setup().click(screen.getByRole('button', { name: 'Try again' }));
    expect(onRetryAction).toHaveBeenCalledTimes(1);
  });

  it('states why Submit is disabled', () => {
    setup({ text: '  ' });
    expect(screen.getByRole('button', { name: 'Submit response' })).toBeDisabled();
    expect(screen.getByText('Write a response to submit it.')).toBeVisible();
  });

  it('reports typing', async () => {
    const props = setup({ text: '' });
    await userEvent.setup().type(screen.getByRole('textbox', { name: 'Your response' }), 'a');
    expect(props.onChange).toHaveBeenCalledWith('a');
  });
});
