/**
 * Unsent text, kept by where it was typed rather than by the component that
 * shows it. A composer unmounts whenever its pane does (a route change, a
 * folder switch, a closed sidebar), and text held in its own state went with
 * it. Here it waits until the same place is opened again, and survives a
 * restart: a draft is only gone once it is sent, emptied or its conversation
 * deleted.
 */

import { useCallback, useState } from 'react';

import { create } from 'zustand';

const STORAGE_KEY = 'lattice.drafts';

type Drafts = Readonly<Record<string, string>>;

function readStored(): Drafts {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter((entry): entry is [string, string] => typeof entry[1] === 'string' && entry[1] !== ''),
    );
  } catch {
    return {};
  }
}

function writeStored(drafts: Drafts): void {
  try {
    if (Object.keys(drafts).length === 0) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, JSON.stringify(drafts));
  } catch {
    // The draft still lives for this session.
  }
}

export const useDraftStore = create<{ drafts: Drafts }>(() => ({ drafts: readStored() }));

/** Every composer that sends into a conversation shares its draft: chat, Explorer and tangents. */
export const conversationDraftKey = (conversationId: string): string => `conversation:${conversationId}`;

export function setDraft(key: string, value: string): void {
  const { drafts } = useDraftStore.getState();
  if ((drafts[key] ?? '') === value) return;
  const { [key]: _previous, ...rest } = drafts;
  const next = value === '' ? rest : { ...rest, [key]: value };
  useDraftStore.setState({ drafts: next });
  writeStored(next);
}

export function clearDraft(key: string): void {
  setDraft(key, '');
}

export function readDraft(key: string): string {
  return useDraftStore.getState().drafts[key] ?? '';
}

/**
 * The draft kept under `key`, and its setter. With no key (nothing to file the
 * text under yet) the text lives in the component, as before.
 */
export function useDraft(key: string | null): [string, (value: string) => void] {
  const stored = useDraftStore((state) => (key ? state.drafts[key] : undefined) ?? '');
  const [local, setLocal] = useState('');
  const set = useCallback((value: string) => {
    if (key) setDraft(key, value);
    else setLocal(value);
  }, [key]);
  return [key ? stored : local, set];
}
