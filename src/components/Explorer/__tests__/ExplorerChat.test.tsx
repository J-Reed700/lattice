import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { ExplorerChat } from '../ExplorerChat';

import type { ExplorerThreads } from '../useExplorerThread';

vi.mock('@/features/chat/components/ChatPanel', () => ({ ChatPanel: () => <div>Conversation messages</div> }));

function threads(remove = vi.fn().mockResolvedValue(true)): ExplorerThreads {
  return {
    threads: [{ id: 'thread-1', title: 'Project questions', updatedAt: '2026-10-08T12:00:00Z' }],
    activeId: 'thread-1',
    preparing: false,
    error: null,
    select: vi.fn(),
    create: vi.fn(),
    remove,
    retry: vi.fn(),
  };
}

function show(value: ExplorerThreads) {
  return render(<TooltipProvider><ExplorerChat threads={value} /></TooltipProvider>);
}

describe('ExplorerChat conversation actions', () => {
  it('deletes the active conversation after confirmation', async () => {
    const user = userEvent.setup();
    const remove = vi.fn().mockResolvedValue(true);
    show(threads(remove));

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation…' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('Project questions');
    expect(remove).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Delete conversation' }));
    expect(remove).toHaveBeenCalledWith('thread-1');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('keeps the confirmation open when deletion fails', async () => {
    const user = userEvent.setup();
    show(threads(vi.fn().mockResolvedValue(false)));

    await user.click(screen.getByRole('button', { name: 'Conversation actions' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation…' }));
    await user.click(screen.getByRole('button', { name: 'Delete conversation' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be deleted');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
});
