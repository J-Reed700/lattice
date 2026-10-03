/**
 * The Explorer: a folder from disk beside a chat.
 *
 * One store for the page and for the chat code that has to know what the
 * Explorer shows (the send path reads the focus, answers reveal references).
 * The folder, the recent folders and the thread last used per folder survive
 * a restart; what is expanded, open and selected belongs to the session.
 */

import { create } from 'zustand';

const ROOT_KEY = 'explorer.root';
const RECENT_KEY = 'explorer.recentRoots';
const THREADS_KEY = 'explorer.threadByRoot';
const MAX_RECENT = 8;

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
 */
export interface FolderIndexStatus {
  root: string;
  indexRoot: string;
  state: FolderIndexState;
  filesTotal: number;
  filesIndexed: number;
  chunks: number;
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
  recentRoots: ExplorerRoot[];
  /** Directories open in the tree, as scope-relative paths. */
  expanded: ReadonlySet<string>;
  openPath: string | null;
  /** Lines the reader picked in the gutter; sent with the next turn. */
  selection: ExplorerLineRange | null;
  /** Lines an answer pointed at. */
  highlight: ExplorerHighlight | null;
  /** The thread last used for each root, so a folder reopens its own chat. */
  threadByRoot: Record<string, string>;
  /** The open folder's search index; `null` until the backend has said. */
  indexStatus: FolderIndexStatus | null;
  /**
   * Index statuses taken from events. A command's result is a snapshot from
   * when it ran; an event that arrived meanwhile is newer, so a caller drops
   * the result when this moved while it waited.
   */
  indexEvents: number;

  setRoot: (_root: ExplorerRoot | null) => void;
  toggleExpanded: (_path: string) => void;
  setExpanded: (_path: string, _open: boolean) => void;
  openFile: (_path: string) => void;
  setSelection: (_range: ExplorerLineRange | null) => void;
  /** Open a file and light up the lines a line reference names. */
  reveal: (_path: string, _range: ExplorerLineRange) => void;
  rememberThread: (_root: string, _conversationId: string) => void;
  /** Takes an index status from a command or an event; one for another root is dropped. */
  setIndexStatus: (_status: FolderIndexStatus | null, _fromEvent?: boolean) => void;
  /** Drops a folder from the recent list (its index is deleted separately). */
  forgetRecent: (_root: string) => void;
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

let nonce = 0;

export const useExplorerStore = create<ExplorerState>((set, get) => ({
  root: read<ExplorerRoot | null>(ROOT_KEY, null),
  recentRoots: read<ExplorerRoot[]>(RECENT_KEY, []),
  expanded: new Set<string>(),
  openPath: null,
  selection: null,
  highlight: null,
  threadByRoot: read<Record<string, string>>(THREADS_KEY, {}),
  indexStatus: null,
  indexEvents: 0,

  setRoot: (root) => {
    if (root?.root === get().root?.root) return;
    const recentRoots = root
      ? [root, ...get().recentRoots.filter((item) => item.root !== root.root)].slice(0, MAX_RECENT)
      : get().recentRoots;
    write(ROOT_KEY, root);
    write(RECENT_KEY, recentRoots);
    // Paths are relative to the root, so nothing about the old view carries over.
    set({ root, recentRoots, expanded: new Set(), openPath: null, selection: null, highlight: null, indexStatus: null });
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

  openFile: (path) => {
    if (get().openPath === path) return;
    set({ openPath: path, selection: null, highlight: null });
  },

  setSelection: (selection) => set({ selection }),

  reveal: (path, range) => {
    const expanded = new Set(get().expanded);
    for (const dir of ancestors(path)) expanded.add(dir);
    nonce += 1;
    set({
      expanded,
      // A selection belongs to the file it was made in.
      ...(get().openPath === path ? {} : { openPath: path, selection: null }),
      highlight: { path, range, nonce },
    });
  },

  rememberThread: (root, conversationId) => {
    if (get().threadByRoot[root] === conversationId) return;
    const threadByRoot = { ...get().threadByRoot, [root]: conversationId };
    write(THREADS_KEY, threadByRoot);
    set({ threadByRoot });
  },

  setIndexStatus: (indexStatus, fromEvent = false) => {
    if (indexStatus && indexStatus.root !== get().root?.root) return;
    set(fromEvent ? { indexStatus, indexEvents: get().indexEvents + 1 } : { indexStatus });
  },

  forgetRecent: (root) => {
    const recentRoots = get().recentRoots.filter((item) => item.root !== root);
    write(RECENT_KEY, recentRoots);
    set({ recentRoots });
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
