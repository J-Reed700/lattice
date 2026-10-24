/**
 * The Explorer: a folder from disk beside a chat.
 *
 * One store for the page, which also serves Chat as its folder-thread host
 * (the send path reads the focus, answers reveal references).
 * The open folder survives a restart; what is expanded, open and selected
 * belongs to the session. The folders picked before, the thread each last
 * showed and their index statuses are the backend's, kept in React Query.
 */

import { create } from 'zustand';

import { setFolderThreadHost } from '@/features/chat/model/folderThreadHost';

const ROOT_KEY = 'explorer.root';

/** Lines of a file, 1-based and inclusive, as the backend's `ExplorerLineRangeDto`. */
export interface ExplorerLineRange {
  startLine: number;
  endLine: number;
}

/** What the Explorer shows, as the backend's `ExplorerFocusDto`. */
export interface ExplorerFocus {
  openPath: string | null;
  selection: ExplorerLineRange | null;
}

/** A folder after canonicalisation, as the backend's `ExplorerRootDto`. */
export interface ExplorerRoot {
  root: string;
  name: string;
}

/** Where a folder's search index stands, as the backend's `FolderIndexState`. */
export type FolderIndexState = 'scanning' | 'indexing' | 'ready' | 'tooLarge' | 'refused' | 'unavailable' | 'error';

/**
 * The open folder's search index, as the backend's `FolderIndexStatusDto`.
 * `indexRoot` differs from `root` when a parent folder's index is reused.
 * Passages move after every embedded batch, files when a save marks them;
 * the rate and time left stay null until a run has ~10 s behind it.
 */
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

/** A range to light up in the viewer. `nonce` re-scrolls to the same range. */
export interface ExplorerHighlight {
  path: string;
  range: ExplorerLineRange;
  nonce: number;
}

export interface ExplorerState {
  root: ExplorerRoot | null;
  /** Directories open in the tree, as scope-relative paths. */
  expanded: ReadonlySet<string>;
  openPath: string | null;
  /** Files opened before the one on screen, the most recent last. */
  back: readonly string[];
  /** Files left by going back, the next one last. */
  forward: readonly string[];
  /** Lines the reader picked in the gutter; sent with the next turn. */
  selection: ExplorerLineRange | null;
  /** Lines an answer pointed at. */
  highlight: ExplorerHighlight | null;
  /**
   * Cited paths that named no file, each with the file it turned out to
   * mean, so the same link opens that file at once instead of being looked
   * for again. Kept for the open folder only.
   */
  aliases: Readonly<Record<string, string>>;

  setRoot: (_root: ExplorerRoot | null) => void;
  toggleExpanded: (_path: string) => void;
  setExpanded: (_path: string, _open: boolean) => void;
  openFile: (_path: string) => void;
  /** Reopen the file before this one, as a browser's Back does. */
  goBack: () => void;
  /** Reopen the file Back left, until another file is opened. */
  goForward: () => void;
  /**
   * The open path named no file, and `to` is the one it meant: show that
   * instead, at the same lines, without a step in the history, and remember
   * it for the next link that cites `from`.
   */
  retarget: (_from: string, _to: string) => void;
  setSelection: (_range: ExplorerLineRange | null) => void;
  /** Open a file and light up the lines a line reference names. */
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
    // Preference only.
  }
}

/** Every directory above `path`, so revealing a file opens its way down. */
function ancestors(path: string): string[] {
  const parts = path.split('/');
  return parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join('/'));
}

/** `expanded` with the folders above `path` open, or the same set when they already are. */
function withAncestors(expanded: ReadonlySet<string>, path: string): ReadonlySet<string> {
  const missing = ancestors(path).filter((dir) => !expanded.has(dir));
  return missing.length ? new Set([...expanded, ...missing]) : expanded;
}

/** How many files Back remembers. */
const HISTORY_LIMIT = 50;

/** Leaving the open file for another: it goes on the Back list, and Forward is spent. */
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
    // Paths are relative to the root, so nothing about the old view carries over.
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
    // A remembered file that has since gone points its citers at the new one.
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
      // A selection belongs to the file it was made in.
      ...(get().openPath === path ? {} : { ...leaving(get()), openPath: path, selection: null }),
      highlight: { path, range, nonce },
    });
  },
}));

export const explorerStore = useExplorerStore;

/**
 * What a turn from an Explorer conversation tells the model is on screen.
 * Null when nothing is open, so the backend does not get an empty focus.
 */
export function currentExplorerFocus(): ExplorerFocus | null {
  const { openPath, selection } = useExplorerStore.getState();
  if (!openPath) return null;
  return { openPath, selection };
}

// Chat reaches the Explorer only through this host: a turn from a folder's
// thread carries the focus while the Explorer shows that folder, and an
// answer's line references open here.
setFolderThreadHost({
  focusFor: (root) => (useExplorerStore.getState().root?.root === root ? currentExplorerFocus() : null),
  reveal: (path, range) => useExplorerStore.getState().reveal(path, range),
  openFile: (path) => useExplorerStore.getState().openFile(path),
});
