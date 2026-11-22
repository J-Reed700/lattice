import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useToastStore } from '@/stores/toastStore';
import { deferred } from '@/tests/deferred';

import { InstalledDownload } from './InstalledDownload';

const titles = () => useToastStore.getState().toasts.map((t) => t.title);

function setup(onRemove: () => Promise<unknown>) {
  render(<InstalledDownload name="the reranker" consequence="Search loses reranking." size="639 MB" onRemove={onRemove} />);
  return userEvent.setup();
}

beforeEach(() => useToastStore.setState({ toasts: [] }));

describe('InstalledDownload', () => {
  it('shows Downloaded and a Remove button', () => {
    setup(vi.fn());
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeEnabled();
  });

  it('explains the consequence and what it frees before removing', async () => {
    const user = setup(vi.fn());
    await user.click(screen.getByRole('button', { name: 'Remove the reranker' }));
    expect(await screen.findByText('Remove the reranker?')).toBeInTheDocument();
    expect(screen.getByText('Search loses reranking. This frees 639 MB. You can download it again here.')).toBeInTheDocument();
  });

  it('leaves out what it frees when the size is unknown', async () => {
    render(<InstalledDownload name="the reranker" consequence="Search loses reranking." onRemove={vi.fn()} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove the reranker' }));
    expect(await screen.findByText('Search loses reranking. You can download it again here.')).toBeInTheDocument();
  });

  it('does not remove when cancelled', async () => {
    const onRemove = vi.fn(async () => {});
    const user = setup(onRemove);
    await user.click(screen.getByRole('button', { name: 'Remove the reranker' }));
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByText('Remove the reranker?')).toBeNull());
    expect(onRemove).not.toHaveBeenCalled();
  });

  it('removes once on confirm, shows Removing… meanwhile, and toasts', async () => {
    const call = deferred<void>();
    const onRemove = vi.fn(() => call.promise);
    const user = setup(onRemove);
    await user.click(screen.getByRole('button', { name: 'Remove the reranker' }));
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    expect(onRemove).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole('button', { name: 'Remove the reranker' })).toHaveTextContent('Removing…');
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeDisabled();

    call.resolve();
    await waitFor(() => expect(titles()).toContain('Removed the reranker'));
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeEnabled();
  });

  it('toasts the error and re-enables the button when removal fails', async () => {
    const user = setup(vi.fn(async () => { throw new Error('Files are in use.'); }));
    await user.click(screen.getByRole('button', { name: 'Remove the reranker' }));
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    await waitFor(() => expect(titles()).toContain("Couldn't remove the reranker"));
    expect(useToastStore.getState().toasts.some((t) => t.message === 'Files are in use.')).toBe(true);
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeEnabled();
  });
});
