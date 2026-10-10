import type { ReactNode } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { ExplorerThreads } from '@/features/explorer/hooks/useExplorerThread';

import { ExplorerChat } from '../ExplorerChat';


vi.mock('@/features/chat/components/ChatPanel', () => ({
  ChatPanel: ({ composerNotice, composerChips }: { composerNotice?: ReactNode; composerChips?: ReactNode }) => (
    <div>
      Conversation messages
      <div data-testid="composer-notice">{composerNotice}</div>
      <div data-testid="composer-chips">{composerChips}</div>
    </div>
  ),
}));
vi.mock('../ExplorerIndexNotice', () => ({ ExplorerIndexNotice: () => <p>Folder index notice</p> }));
vi.mock('../ExplorerSelectionChip', () => ({ ExplorerSelectionChip: () => <span>Selected lines chip</span> }));

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

describe('ExplorerChat composer', () => {
  it('puts the folder index notice and the selected lines into the chat composer', () => {
    show(threads());
    expect(screen.getByTestId('composer-notice')).toHaveTextContent('Folder index notice');
    expect(screen.getByTestId('composer-chips')).toHaveTextContent('Selected lines chip');
  });
});
