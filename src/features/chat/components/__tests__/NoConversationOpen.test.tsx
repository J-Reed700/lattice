import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { NoConversationOpen } from '@/features/chat/components/NoConversationOpen';

describe('NoConversationOpen', () => {
  it('starts a conversation, and waits while one starts', () => {
    const onCreate = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(<NoConversationOpen isCreatingConversation={false} onCreate={onCreate} />);
    expect(screen.getByText('No conversation open.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Create new conversation' }));
    expect(onCreate).toHaveBeenCalled();
    rerender(<NoConversationOpen isCreatingConversation onCreate={onCreate} />);
    expect(screen.getByRole('button', { name: 'Create new conversation' })).toBeDisabled();
  });
});
