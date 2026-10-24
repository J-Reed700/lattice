import { create } from 'zustand';

import { setFolderThreadHost } from '@/features/chat/model/folderThreadHost';

const ROOT_KEY = 'explorer.root';

export interface ExplorerLineRange {
  startLine: number;
  endLine: number;
}

export interface ExplorerFocus {
  openPath: string | null;
  selection: ExplorerLineRange | null;
}

export interface ExplorerRoot {
  root: string;
  name: string;
}

export type FolderIndexState = 'scanning' | 'indexing' | 'ready' | 'tooLarge' | 'refused' | 'unavailable' | 'error';

export interface FolderIndexStatus {
  root: string;
  indexRoot: string;
  state: FolderIndexState;
  filesTotal: number;
  filesIndexed: number;
  passagesTotal: number;
  passagesEmbedded: number;
  passagesPerSecond: number | null;
  etaSeconds: number | null;
  message: string | null;
}

export interface ExplorerHighlight {
  path: string;
  range: ExplorerLineRange;
  nonce: number;
}

export interface ExplorerState {
  root: ExplorerRoot | null;
  expanded: ReadonlySet<string>;
  openPath: string | null;
  back: readonly string[];
  forward: readonly string[];
  selection: ExplorerLineRange | null;
  highlight: ExplorerHighlight | null;
  aliases: Readonly<Record<string, string>>;

  setRoot: (_root: ExplorerRoot | null) => void;
  toggleExpanded: (_path: string) => void;
  setExpanded: (_path: string, _open: boolean) => void;
  openFile: (_path: string) => void;
  goBack: () => void;
  goForward: () => void;
  retarget: (_from: string, _to: string) => void;
  setSelection: (_range: ExplorerLineRange | null) => void;
  reveal: (_path: string, _range: ExplorerLineRange) => void;
}

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Ignore storage errors
  }
}

function ancestors(path: string): string[] {
  const parts = path.split('/');
  return parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join('/'));
}

function withAncestors(expanded: ReadonlySet<string>, path: string): ReadonlySet<string> {
  const missing = ancestors(path).filter((dir) => !expanded.has(dir));
  return missing.length ? new Set([...expanded, ...missing]) : expanded;
}

const HISTORY_LIMIT = 50;

function leaving(state: ExplorerState): Pick<ExplorerState, 'back' | 'forward'> {
  return {
    back: state.openPath ? [...state.back, state.openPath].slice(-HISTORY_LIMIT) : state.back,
    forward: [],
  };
}

let nonce = 0;

export const useExplorerStore = create<ExplorerState>((set, get) => ({
  root: read<ExplorerRoot | null>(ROOT_KEY, null),
  expanded: new Set<string>(),
  openPath: null,
  back: [],
  forward: [],
  selection: null,
  highlight: null,
  aliases: {},

  setRoot: (root) => {
    if (root?.root === get().root?.root) return;
    write(ROOT_KEY, root);
    set({ root, expanded: new Set(), openPath: null, back: [], forward: [], selection: null, highlight: null, aliases: {} });
  },

  toggleExpanded: (path) => {
    const expanded = new Set(get().expanded);
    if (expanded.has(path)) expanded.delete(path);
    else expanded.add(path);
    set({ expanded });
  },

  setExpanded: (path, open) => {
    if (get().expanded.has(path) === open) return;
    const expanded = new Set(get().expanded);
    if (open) expanded.add(path);
    else expanded.delete(path);
    set({ expanded });
  },

  openFile: (cited) => {
    const path = get().aliases[cited] ?? cited;
    if (get().openPath === path) return;
    set({ ...leaving(get()), openPath: path, selection: null, highlight: null });
  },

  goBack: () => {
    const { back, forward, openPath, expanded } = get();
    const path = back[back.length - 1];
    if (!path) return;
    set({
      back: back.slice(0, -1),
      forward: openPath ? [...forward, openPath] : forward,
      openPath: path,
      expanded: withAncestors(expanded, path),
      selection: null,
      highlight: null,
    });
  },

  goForward: () => {
    const { back, forward, openPath, expanded } = get();
    const path = forward[forward.length - 1];
    if (!path) return;
    set({
      back: openPath ? [...back, openPath] : back,
      forward: forward.slice(0, -1),
      openPath: path,
      expanded: withAncestors(expanded, path),
      selection: null,
      highlight: null,
    });
  },

  retarget: (from, to) => {
    const { openPath, highlight, expanded, aliases } = get();
    if (openPath !== from || from === to) return;
    const remembered = Object.fromEntries(
      Object.entries(aliases).map(([cited, file]) => [cited, file === from ? to : file]),
    );
    set({
      openPath: to,
      expanded: withAncestors(expanded, to),
      highlight: highlight?.path === from ? { ...highlight, path: to } : highlight,
      aliases: { ...remembered, [from]: to },
    });
  },

  setSelection: (selection) => set({ selection }),

  reveal: (cited, range) => {
    const path = get().aliases[cited] ?? cited;
    const expanded = new Set(get().expanded);
    for (const dir of ancestors(path)) expanded.add(dir);
    nonce += 1;
    set({
      expanded,
      ...(get().openPath === path ? {} : { ...leaving(get()), openPath: path, selection: null }),
      highlight: { path, range, nonce },
    });
  },
}));

export const explorerStore = useExplorerStore;

export function currentExplorerFocus(): ExplorerFocus | null {
  const { openPath, selection } = useExplorerStore.getState();
  if (!openPath) return null;
  return { openPath, selection };
}

setFolderThreadHost({
  focusFor: (root) => (useExplorerStore.getState().root?.root === root ? currentExplorerFocus() : null),
  reveal: (path, range) => useExplorerStore.getState().reveal(path, range),
  openFile: (path) => useExplorerStore.getState().openFile(path),
});
