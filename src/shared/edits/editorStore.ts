import { create } from 'zustand';

import type { ContentAddress, EditSource, JsonValue } from './types';

export type EditorAim = { lines: [number, number]; source: string; sourceStart: number };

export type EditorOptions = {
  aim?: EditorAim;
  initialValues?: Record<string, JsonValue>;
  editSource?: EditSource;
  returnFocusTo?: HTMLElement | null;
};

export type EditorRequest = { address: ContentAddress; options: EditorOptions };

type EditorState = {
  current: EditorRequest | null;
  next: EditorRequest | null;
};

export const useEditorStore = create<EditorState>(() => ({
  current: null,
  next: null,
}));

export function openEditor(address: ContentAddress, options: EditorOptions = {}): void {
  const request: EditorRequest = { address, options };
  useEditorStore.setState((state) => (
    state.current === null
      ? { current: request, next: null }
      : { current: state.current, next: request }
  ));
}

export function closeEditor(): void {
  useEditorStore.setState((state) => ({
    current: state.next,
    next: null,
  }));
}
