import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { UserPictures } from '@/features/chat/components/message/UserPictures';

const restagePicture = vi.hoisted(() => vi.fn().mockResolvedValue(true));
vi.mock('@/features/chat/images/stagedPictures', () => ({ restagePicture }));
vi.mock('@/features/chat/images/chatImageUrl', () => ({
  chatImageUrl: (image: { fileName: string }) => `lattice-image://localhost/${image.fileName}`,
}));

const picture = (n: number) => ({
  id: String(n).repeat(32), ref: `att-${n}`, fileName: `${String(n).repeat(32)}.jpg`, mime: 'image/jpeg',
  width: 800, height: 600, byteSize: 1000, caption: `p${n}.jpg`, sourceUrl: null, sourceTitle: null,
  origin: 'attachment' as const, license: null, attribution: null, originalName: `photo-${n}.jpg`,
});

describe('UserPictures', () => {
  it('shows each picture from the stored file, with no citation or source link', () => {
    render(<UserPictures pictures={[picture(1), picture(2)]} picturesSeen answeredBy={null} restageIn="conversation-1" />);

    const shown = screen.getAllByRole('img');
    expect(shown.map((image) => image.getAttribute('alt'))).toEqual(['photo-1.jpg', 'photo-2.jpg']);
    expect(shown[0]).toHaveAttribute('src', `lattice-image://localhost/${picture(1).fileName}`);
    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.queryByText(/Not shown/)).toBeNull();
  });

  it('says once, under the row, that a model which cannot see was not shown them', () => {
    render(<UserPictures pictures={[picture(1), picture(2)]} picturesSeen={false} answeredBy="Local 9B" restageIn="conversation-1" />);

    expect(screen.getAllByText('Not shown to Local 9B')).toHaveLength(1);
  });

  it('stages a copy for the next turn when asked to show a picture again', async () => {
    render(<UserPictures pictures={[picture(1)]} picturesSeen={null} answeredBy={null} restageIn="conversation-1" />);

    await userEvent.click(screen.getByRole('button', { name: 'Show to the model again' }));

    expect(restagePicture).toHaveBeenCalledWith('conversation-1', picture(1).id);
  });

  it('offers no restage on a question that is not saved yet, or while the chat model cannot see', () => {
    render(<UserPictures pictures={[picture(1)]} picturesSeen={null} answeredBy={null} restageIn={null} />);

    expect(screen.queryByRole('button', { name: 'Show to the model again' })).toBeNull();
  });
});
