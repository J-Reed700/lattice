import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { ChatDropStaging } from '@/features/chat/components/ChatDropStaging';

const noop = () => {};

describe('ChatDropStaging', () => {
  it('renders nothing when no files are staged', () => {
    const { container } = render(
      <ChatDropStaging staged={[]} isImporting={false} onRemove={noop} onClear={noop} />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('says the files join on send, with no separate add step', () => {
    // Send consumes the staging: the row only names what is queued and offers
    // the ways out, because a separate "add" button was exactly the step
    // people forgot.
    render(
      <TooltipProvider>
        <ChatDropStaging
          staged={[{ path: '/vault/paper.pdf', name: 'paper.pdf' }]}
          isImporting={false}
          onRemove={vi.fn()}
          onClear={vi.fn()}
        />
      </TooltipProvider>
    );

    expect(screen.getByText('1 file — added when you send')).toBeInTheDocument();
    expect(screen.queryByText('Add to this conversation')).not.toBeInTheDocument();
  });
});
