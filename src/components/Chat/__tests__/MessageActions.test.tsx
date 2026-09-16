import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { MessageActions } from '../MessageActions';

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
});
