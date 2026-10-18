import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { ChatDropStaging } from '@/features/chat/components/ChatDropStaging';

const noop = () => {};
vi.mock('@/features/chat/images/chatImageUrl', () => ({
  chatImageUrl: (image: { fileName: string }) => `lattice-image://localhost/${image.fileName}`,
}));

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

  describe('pictures', () => {
    const picture = (n: number) => ({
      id: String(n).repeat(32), ref: `att-${n}`, fileName: `${String(n).repeat(32)}.jpg`, mime: 'image/jpeg',
      width: 800, height: 600, byteSize: 1000, caption: `p${n}.jpg`, sourceUrl: null, sourceTitle: null,
      origin: 'attachment' as const, license: null, attribution: null, originalName: `photo-${n}.jpg`,
    });

    it('shows staged pictures as thumbnails, each removable', async () => {
      const onRemovePicture = vi.fn();
      render(
        <TooltipProvider>
          <ChatDropStaging staged={[]} isImporting={false} onRemove={noop} onClear={noop}
            pictures={[picture(1), picture(2)]} onRemovePicture={onRemovePicture} />
        </TooltipProvider>
      );

      expect(screen.getByRole('list', { name: 'Pictures for your next message' })).toBeInTheDocument();
      expect(screen.getByText('2 pictures — added when you send')).toBeInTheDocument();
      await userEvent.click(screen.getByRole('button', { name: 'Remove photo-2.jpg' }));
      expect(onRemovePicture).toHaveBeenCalledWith(picture(2).id);
      expect(screen.queryByRole('note')).toBeNull();
    });

    it('says when the model answering cannot see them', () => {
      render(
        <MemoryRouter>
          <TooltipProvider>
            <ChatDropStaging staged={[]} isImporting={false} onRemove={noop} onClear={noop}
              pictures={[picture(1)]} onRemovePicture={noop} blindModelName="Local 9B" />
          </TooltipProvider>
        </MemoryRouter>
      );

      expect(screen.getByRole('note')).toHaveTextContent("Local 9B can't see pictures. It will be told they're attached, but won't see them.");
      expect(screen.getByRole('link', { name: 'Vision settings' })).toHaveAttribute('href', '/settings?tab=downloaded-models');
    });
  });
});
