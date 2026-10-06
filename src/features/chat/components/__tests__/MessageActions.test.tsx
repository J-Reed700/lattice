import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it, vi } from 'vitest';

import { MessageActions } from '@/features/chat/components/MessageActions';
import { TangentSelectionContext } from '@/features/chat/components/tangents/TangentSelection';
import type { SourceWithMetadata } from '@/types/conversation';


vi.mock('@/hooks/useDownloadedModels', () => ({
  useDownloadedModels: () => ({ downloadedModels: [] }),
}));

const noop = () => {};

const baseProps = {
  canBookmark: true,
  canDelete: true,
  canBranch: true,
  isBookmarked: false,
  isBusy: false,
  onCopy: noop,
  onBookmarkToggle: noop,
  onDelete: noop,
  onRegenerate: noop,
  onTryWithModel: noop,
  onEdit: noop,
  onBranch: noop,
};

const answerSource = {
  documentId: 'doc-halvorsen',
  chunkId: 'doc-halvorsen#c1',
  fileName: 'Halvorsen 2024.pdf',
  filePath: '/vault/Halvorsen 2024.pdf',
  mimeType: 'application/pdf',
  category: 'Research Paper',
  content: 'A cited passage.',
  score: 0.9,
  fileSizeBytes: 100,
  modifiedAt: '2026-08-01T00:00:00.000Z',
} as SourceWithMetadata;

describe('MessageActions', () => {
  it('offers regenerate and try-with on the last assistant turn', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn />);
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
    expect(screen.getByText('Try with another model')).toBeInTheDocument();
    expect(screen.getByText('Branch here')).toBeInTheDocument();
    expect(screen.getByText('Reference')).toBeInTheDocument();
  });

  it('offers neither on an earlier assistant turn', () => {
    // Regenerating mid-thread would silently discard everything after it.
    render(<MessageActions {...baseProps} role="assistant" isLastTurn={false} />);
    expect(screen.queryByText('Regenerate')).not.toBeInTheDocument();
    expect(screen.queryByText('Try with another model')).not.toBeInTheDocument();
    expect(screen.getByText('Branch here')).toBeInTheDocument();
  });

  it('offers Edit on a user turn, and neither regenerate nor try-with', () => {
    render(<MessageActions {...baseProps} role="user" isLastTurn />);
    expect(screen.getByText('Edit')).toBeInTheDocument();
    expect(screen.queryByText('Regenerate')).not.toBeInTheDocument();
    expect(screen.queryByText('Try with another model')).not.toBeInTheDocument();
    expect(screen.queryByText('Reference')).not.toBeInTheDocument();
  });

  it('never offers Edit on an assistant turn', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn />);
    expect(screen.queryByText('Edit')).not.toBeInTheDocument();
  });

  it('hides Branch here when branching is not possible', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn canBranch={false} />);
    expect(screen.queryByText('Branch here')).not.toBeInTheDocument();
  });

  it('disables every mutating verb while a generation is in flight', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn isBusy />);
    expect(screen.getByLabelText('Regenerate this answer')).toBeDisabled();
    expect(screen.getByLabelText('Try this question with another model')).toBeDisabled();
    expect(screen.getByLabelText('Branch the conversation here')).toBeDisabled();
    expect(screen.getByLabelText('Delete message')).toBeDisabled();
    // Copy is read-only and stays available.
    expect(screen.getByLabelText('Copy message to clipboard')).not.toBeDisabled();
  });

  it('shows Referenced once a message is saved', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn isBookmarked />);
    expect(screen.getByText('Referenced')).toBeInTheDocument();
  });

  it('stays visible without a hover, and keeps a focus ring', () => {
    // CHAT-POLISH-COMPONENTS §5: low contrast and full opacity always. Verbs
    // that only exist on hover teach nobody they exist.
    const { container } = render(
      <MessageActions {...baseProps} role="assistant" isLastTurn />
    );
    const row = container.firstElementChild as HTMLElement;
    expect(row.className).not.toContain('opacity-0');
    expect(row.className).not.toContain('group-hover:opacity-100');
    expect(screen.getByLabelText('Copy message to clipboard').className).toContain(
      'focus-visible:ring-2'
    );
  });

  it('starts a tangent from the visible reply action without selecting text', () => {
    const create = vi.fn();
    const tangentSource = { conversationId: 'parent', messageId: 'reply', getText: () => 'The whole reply.' };
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <MessageActions {...baseProps} role="assistant" isLastTurn tangentSource={tangentSource} />
    </TangentSelectionContext.Provider>);
    const button = screen.getByRole('button', { name: 'Start a tangent from this reply' });
    expect(button).toBeVisible();
    expect(button).toHaveTextContent('Tangent');
    fireEvent.click(button);
    expect(create).toHaveBeenCalledWith({ conversationId: 'parent', messageId: 'reply', selectedText: 'The whole reply.' });
  });

  it('lets a long reply be narrowed without requiring text selection or silently truncating it', async () => {
    const create = vi.fn();
    const longReply = 'A long reply. '.repeat(700);
    const tangentSource = { conversationId: 'parent', messageId: 'reply', getText: () => longReply };
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <MessageActions {...baseProps} role="assistant" isLastTurn tangentSource={tangentSource} />
    </TangentSelectionContext.Provider>);
    fireEvent.click(screen.getByRole('button', { name: 'Start a tangent from this reply' }));
    expect(create).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog', { name: 'Choose a starting passage' })).toBeVisible();
    const passage = screen.getByRole('textbox', { name: 'Passage to explore' });
    expect(passage).toHaveValue(longReply.trim());
    expect(screen.getByRole('button', { name: 'Start tangent' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start a tangent from this reply' })).toHaveFocus());
    expect(create).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Start a tangent from this reply' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Passage to explore' }), { target: { value: 'The part I want to discuss.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Start tangent' }));
    expect(create).toHaveBeenCalledWith({ conversationId: 'parent', messageId: 'reply', selectedText: 'The part I want to discuss.' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('does not offer a tangent on a user message and waits for pending creation', () => {
    const create = vi.fn();
    const tangentSource = { conversationId: 'parent', messageId: 'reply', getText: () => 'Reply' };
    const content = (role: string, creating: boolean) => <TangentSelectionContext.Provider value={{ create, creating }}>
      <MessageActions {...baseProps} role={role} isLastTurn tangentSource={tangentSource} />
    </TangentSelectionContext.Provider>;
    const view = render(content('user', false));
    expect(screen.queryByRole('button', { name: 'Start a tangent from this reply' })).not.toBeInTheDocument();
    view.rerender(content('assistant', true));
    expect(screen.getByRole('button', { name: 'Start a tangent from this reply' })).toBeDisabled();
    expect(create).not.toHaveBeenCalled();
  });

  it('offers a way to carry a grounded answer out of the chat', () => {
    // The verbs that leave the chat sit in the row like every other one: a menu
    // that only exists on hover teaches nobody it is there.
    render(
      <MemoryRouter>
        <MessageActions
          {...baseProps}
          role="assistant"
          isLastTurn
          answerSources={[answerSource]}
          answerMarkdown="The pooled estimate was 1.2 °C [1]."
        />
      </MemoryRouter>
    );

    expect(screen.getByText('Use this answer')).toBeInTheDocument();
  });

  it('offers nothing to carry out of an answer with no sources behind it', () => {
    render(<MessageActions {...baseProps} role="assistant" isLastTurn answerMarkdown="Just prose." />);
    expect(screen.queryByText('Use this answer')).not.toBeInTheDocument();
  });

  it('never offers it on a question', () => {
    render(
      <MemoryRouter>
        <MessageActions
          {...baseProps}
          role="user"
          isLastTurn
          answerSources={[answerSource]}
          answerMarkdown="What do my sources say?"
        />
      </MemoryRouter>
    );

    expect(screen.queryByText('Use this answer')).not.toBeInTheDocument();
  });
});
