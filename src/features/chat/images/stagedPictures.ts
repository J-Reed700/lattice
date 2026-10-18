/**
 * Pictures on a draft. Each is stored as a row the moment it is staged, so
 * the composer can show it from the `lattice-image` scheme; the message that
 * sends it claims it, and a draft that drops it releases it.
 *
 * Keyed by conversation, so a picture staged from a sent bubble ("Show to the
 * model again") lands on that chat's composer.
 */

import { create } from 'zustand';

import { parseImages } from '@/features/chat/controller/messageMetadata';
import { VaultAPI } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { ChatImage } from '@/types/conversation';

interface StagedPicturesState {
  byConversation: Readonly<Record<string, readonly ChatImage[]>>;
  add: (conversationId: string, pictures: readonly ChatImage[]) => void;
  remove: (conversationId: string, ids: readonly string[]) => void;
}

export const useStagedPicturesStore = create<StagedPicturesState>((set) => ({
  byConversation: {},
  add: (conversationId, pictures) => set((state) => {
    if (pictures.length === 0) return state;
    const current = state.byConversation[conversationId] ?? [];
    const fresh = pictures.filter((picture) => !current.some((staged) => staged.id === picture.id));
    return { byConversation: { ...state.byConversation, [conversationId]: [...current, ...fresh] } };
  }),
  remove: (conversationId, ids) => set((state) => {
    const current = state.byConversation[conversationId];
    if (!current) return state;
    const { [conversationId]: _dropped, ...rest } = state.byConversation;
    const kept = current.filter((picture) => !ids.includes(picture.id));
    return { byConversation: kept.length > 0 ? { ...rest, [conversationId]: kept } : rest };
  }),
}));

const NONE: readonly ChatImage[] = [];

/** The pictures staged on a conversation's draft. */
export function useStagedPictures(conversationId: string | null): readonly ChatImage[] {
  return useStagedPicturesStore((state) => (conversationId ? state.byConversation[conversationId] : undefined) ?? NONE);
}

/** The file types `attachMessageImages` takes; anything else is imported as a document. */
const PICTURE_PATH = /\.(?:png|jpe?g|gif|webp)$/i;

export const isPicturePath = (path: string): boolean => PICTURE_PATH.test(path);

/** Stores picture files on the draft. Returns whether they were stored. */
export async function attachPictures(conversationId: string, paths: string[]): Promise<boolean> {
  if (paths.length === 0) return true;
  const result = await VaultAPI.attachMessageImages(conversationId, paths);
  if (!result.ok) {
    toast.error(paths.length === 1 ? "Couldn't attach this picture" : "Couldn't attach these pictures", {
      message: result.error,
    });
    return false;
  }
  useStagedPicturesStore.getState().add(conversationId, parseImages(result.data));
  return true;
}

/** Drops pictures from the draft and lets the backend delete their rows. */
export async function releasePictures(conversationId: string, ids: readonly string[]): Promise<void> {
  if (ids.length === 0) return;
  useStagedPicturesStore.getState().remove(conversationId, ids);
  // A failed release is reported by the transport's diagnostics; the row
  // stays a draft that no message claims.
  await VaultAPI.releaseMessageImages(conversationId, [...ids]);
}

/** Stages a copy of a sent picture, so the next turn shows the model its pixels again. */
export async function restagePicture(conversationId: string, imageId: string): Promise<boolean> {
  const result = await VaultAPI.restageMessageImage(conversationId, imageId);
  if (!result.ok) {
    toast.error("Couldn't add this picture to your message", { message: result.error });
    return false;
  }
  useStagedPicturesStore.getState().add(conversationId, parseImages([result.data]));
  return true;
}

/** Hands the staged pictures to a send: cleared from the draft, not released. */
export function takePictures(conversationId: string): ChatImage[] {
  const store = useStagedPicturesStore.getState();
  const pictures = [...(store.byConversation[conversationId] ?? [])];
  store.remove(conversationId, pictures.map((picture) => picture.id));
  return pictures;
}
