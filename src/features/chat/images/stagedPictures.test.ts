import { beforeEach, describe, expect, it, vi } from 'vitest';

import { attachPictures, isPicturePath, releasePictures, restagePicture, takePictures, useStagedPicturesStore } from './stagedPictures';

const h = vi.hoisted(() => ({
  api: { attachMessageImages: vi.fn(), releaseMessageImages: vi.fn(), restageMessageImage: vi.fn() },
  toast: { error: vi.fn() },
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const picture = (n: number) => ({
  id: String(n).repeat(32).slice(0, 32).replace(/[^0-9a-f]/g, 'a'), ref: `att-${n}`, fileName: `${'c'.repeat(32)}.png`, mime: 'image/png',
  width: 4, height: 4, byteSize: 1, caption: '', sourceUrl: null, sourceTitle: null, origin: 'attachment', license: null, attribution: null,
});

const staged = (conversationId: string) => useStagedPicturesStore.getState().byConversation[conversationId] ?? [];

describe('stagedPictures', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStagedPicturesStore.setState({ byConversation: {} });
  });

  it('recognises picture files', () => {
    expect(isPicturePath('/photos/heron.JPG')).toBe(true);
    expect(isPicturePath('/photos/heron.webp')).toBe(true);
    expect(isPicturePath('/notes/heron.pdf')).toBe(false);
  });

  it('stores attached pictures on the draft once each', async () => {
    h.api.attachMessageImages.mockResolvedValue({ ok: true, data: [picture(1)] });
    await expect(attachPictures('c1', ['/heron.png'])).resolves.toBe(true);
    await attachPictures('c1', ['/heron.png']);
    expect(staged('c1')).toHaveLength(1);
    expect(h.api.attachMessageImages).toHaveBeenCalledWith('c1', ['/heron.png']);
  });

  it('attaches nothing for no paths', async () => {
    await expect(attachPictures('c1', [])).resolves.toBe(true);
    expect(h.api.attachMessageImages).not.toHaveBeenCalled();
  });

  it('reports pictures that could not be attached, in the singular or plural', async () => {
    h.api.attachMessageImages.mockResolvedValue({ ok: false, error: 'too large' });
    await expect(attachPictures('c1', ['/a.png'])).resolves.toBe(false);
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't attach this picture", { message: 'too large' });
    await attachPictures('c1', ['/a.png', '/b.png']);
    expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't attach these pictures", { message: 'too large' });
  });

  it('releases pictures from the draft and the backend', async () => {
    useStagedPicturesStore.getState().add('c1', [picture(1), picture(2)] as never);
    await releasePictures('c1', [picture(1).id]);
    expect(staged('c1').map((item) => item.ref)).toEqual(['att-2']);
    expect(h.api.releaseMessageImages).toHaveBeenCalledWith('c1', [picture(1).id]);
    await releasePictures('c1', [picture(2).id]);
    expect(useStagedPicturesStore.getState().byConversation).toEqual({});
    await releasePictures('c1', []);
    expect(h.api.releaseMessageImages).toHaveBeenCalledTimes(2);
  });

  it('ignores removals for a conversation with nothing staged, and empty adds', () => {
    const before = useStagedPicturesStore.getState().byConversation;
    useStagedPicturesStore.getState().remove('c9', ['x']);
    useStagedPicturesStore.getState().add('c9', []);
    expect(useStagedPicturesStore.getState().byConversation).toBe(before);
  });

  it('stages a sent picture again, or says why not', async () => {
    h.api.restageMessageImage.mockResolvedValueOnce({ ok: true, data: picture(3) }).mockResolvedValueOnce({ ok: false, error: 'file gone' });
    await expect(restagePicture('c1', 'img')).resolves.toBe(true);
    expect(staged('c1').map((item) => item.ref)).toEqual(['att-3']);
    await expect(restagePicture('c1', 'img')).resolves.toBe(false);
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't add this picture to your message", { message: 'file gone' });
  });

  it('hands the staged pictures to a send and clears them', () => {
    useStagedPicturesStore.getState().add('c1', [picture(1)] as never);
    expect(takePictures('c1').map((item) => item.ref)).toEqual(['att-1']);
    expect(staged('c1')).toEqual([]);
    expect(takePictures('c2')).toEqual([]);
  });
});
