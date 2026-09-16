import { fireEvent, render, screen, act } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import { ConfirmDialog } from './ConfirmDialog';

it('allows cancelling a bulk operation while its current request is pending', async () => {
  let finish!: () => void;
  const pending = new Promise<void>(resolve => { finish = resolve; });
  const onCancel = vi.fn();
  render(<ConfirmDialog isOpen title="Delete documents?" message="Deleting"
    onConfirm={() => pending} onCancel={onCancel} allowCancelWhileLoading />);
  fireEvent.click(screen.getByRole('button', { name: /Confirm/ }));
  const cancel = screen.getByRole('button', { name: 'Cancel' });
  expect(cancel).toBeEnabled();
  fireEvent.click(cancel);
  expect(onCancel).toHaveBeenCalledOnce();
  await act(async () => { finish(); await pending; });
});


it('does not dismiss an operation through Escape while cancellation is disabled', async () => {
  let finish!: () => void;
  const pending = new Promise<void>(resolve => { finish = resolve; });
  const onCancel = vi.fn();
  const onConfirm = vi.fn(() => pending);
  render(<ConfirmDialog isOpen title="Delete?" message="Deleting" onConfirm={onConfirm} onCancel={onCancel} />);
  fireEvent.click(screen.getByRole('button', { name: /Confirm/ }));
  fireEvent.keyDown(window, { key: 'Escape' });
  fireEvent.keyDown(window, { key: 'Enter', metaKey: true });
  expect(onCancel).not.toHaveBeenCalled();
  expect(onConfirm).toHaveBeenCalledOnce();
  await act(async () => { finish(); await pending; });
});

it('shows an action failure and permits retry', async () => {
  const onConfirm = vi.fn().mockRejectedValueOnce(new Error('Disk is full')).mockResolvedValueOnce(undefined);
  const onCancel = vi.fn();
  render(<ConfirmDialog isOpen title="Save?" message="Saving" onConfirm={onConfirm} onCancel={onCancel} />);
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Confirm/ })); });
  expect(screen.getByRole('alert')).toHaveTextContent('Disk is full');
  expect(onCancel).not.toHaveBeenCalled();
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Confirm/ })); });
  expect(onConfirm).toHaveBeenCalledTimes(2);
  expect(onCancel).toHaveBeenCalledOnce();
});
