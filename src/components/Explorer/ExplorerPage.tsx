import { useEffect, useRef, useState } from 'react';

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { Folder, FolderOpen, FolderTree } from 'lucide-react';

import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { VaultAPI } from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore, type ExplorerRoot, type FolderIndexStatus } from '@/stores/explorerStore';
import { toast } from '@/stores/toastStore';
import type { ApiResult } from '@/types';

import { ExplorerChat } from './ExplorerChat';
import { ExplorerFileView } from './ExplorerFileView';
import { ExplorerSearch } from './ExplorerSearch';
import { ExplorerTree } from './ExplorerTree';
import { ScopeBar } from './ScopeBar';
import { SplitHandle } from './SplitHandle';
import { useExplorerThread } from './useExplorerThread';

import './explorer.css';

/** Carries a `FolderIndexStatusDto` whenever the open folder's index moves. */
export const INDEX_STATUS_EVENT = 'explorer-index://status';

const TREE_WIDTH_KEY = 'explorer.treeWidth';
const CHAT_WIDTH_KEY = 'explorer.chatWidth';
const TREE_DEFAULT = 240;
const CHAT_DEFAULT = 440;
const TREE_MIN = 160;
const CHAT_MIN = 340;
/** The file view keeps at least this much between the tree and the chat. */
const VIEWER_MIN = 320;

function readWidth(key: string, fallback: number): number {
  try {
    const value = Number(localStorage.getItem(key));
    return Number.isFinite(value) && value > 0 ? value : fallback;
  } catch {
    return fallback;
  }
}

function writeWidth(key: string, value: number): void {
  try {
    localStorage.setItem(key, String(Math.round(value)));
  } catch {
    // Preference only.
  }
}

/** A command that failed outright, shown the way a failed run is. */
function failedIndex(root: string, message: string): FolderIndexStatus {
  return { root, indexRoot: root, state: 'error', filesTotal: 0, filesIndexed: 0, chunks: 0, message };
}

/**
 * Runs an index command and shows its result, unless an event arrived while
 * it ran: the event is newer than the command's snapshot, and a small folder
 * can finish indexing before the command's reply is back.
 */
async function indexCommand(
  command: () => Promise<ApiResult<FolderIndexStatus>>,
  root: string,
  stillWanted: () => boolean
): Promise<void> {
  const before = useExplorerStore.getState().indexEvents;
  const result = await command();
  if (!stillWanted()) return;
  const { setIndexStatus, indexEvents } = useExplorerStore.getState();
  if (!result.ok) setIndexStatus(failedIndex(root, result.error));
  else if (indexEvents === before) setIndexStatus(result.data);
}

/**
 * Explorer: a folder from disk beside a chat.
 *
 * Tree | file | chat. The chat is the ordinary one, on a thread bound to the
 * folder; it can read the folder, and its line references light up the file.
 */
export function ExplorerPage() {
  const root = useExplorerStore((state) => state.root);
  const recentRoots = useExplorerStore((state) => state.recentRoots);
  const setRoot = useExplorerStore((state) => state.setRoot);
  const indexStatus = useExplorerStore((state) => state.indexStatus);
  const setIndexStatus = useExplorerStore((state) => state.setIndexStatus);
  const forgetRecent = useExplorerStore((state) => state.forgetRecent);
  const loadSpaces = useConversationsStore((state) => state.loadSpaces);
  const { fetchDownloadedModels } = useDownloadedModels();
  const [resolving, setResolving] = useState(false);
  const [searching, setSearching] = useState(false);
  const [treeWidth, setTreeWidth] = useState(() => readWidth(TREE_WIDTH_KEY, TREE_DEFAULT));
  const [chatWidth, setChatWidth] = useState(() => readWidth(CHAT_WIDTH_KEY, CHAT_DEFAULT));
  const [rowWidth, setRowWidth] = useState(0);
  const rowRef = useRef<HTMLDivElement>(null);
  const threads = useExplorerThread(root?.root ?? null, root?.name ?? '');

  // The chat column needs what Chat loads on mount: spaces and models.
  useEffect(() => {
    void loadSpaces();
    void fetchDownloadedModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Whenever a folder is open (the persisted one on launch included), its
  // index is opened too: started, resumed, or simply reported.
  const rootPath = root?.root ?? null;
  useEffect(() => {
    if (!rootPath) return;
    let current = true;
    void indexCommand(() => VaultAPI.explorerIndexOpen(rootPath), rootPath, () => current);
    return () => {
      current = false;
    };
  }, [rootPath]);

  // Progress arrives as events; the store keeps only the open folder's.
  useEffect(() => {
    let mounted = true;
    let unlisten: UnlistenFn | undefined;
    void listen<FolderIndexStatus>(INDEX_STATUS_EVENT, (event) => setIndexStatus(event.payload, true)).then((fn) => {
      if (mounted) unlisten = fn;
      else fn();
    });
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [setIndexStatus]);

  useEffect(() => writeWidth(TREE_WIDTH_KEY, treeWidth), [treeWidth]);
  useEffect(() => writeWidth(CHAT_WIDTH_KEY, chatWidth), [chatWidth]);

  useEffect(() => {
    const element = rowRef.current;
    if (!element || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setRowWidth(element.clientWidth));
    observer.observe(element);
    setRowWidth(element.clientWidth);
    return () => observer.disconnect();
  }, [root]);

  const openRoot = async (path: string) => {
    setResolving(true);
    try {
      const result = await VaultAPI.explorerResolveRoot(path);
      if (!result.ok) {
        toast.error("Couldn't open that folder", { message: result.error });
        return;
      }
      setRoot(result.data);
    } finally {
      setResolving(false);
    }
  };

  const closeFolder = async () => {
    await VaultAPI.explorerIndexClose();
    setRoot(null);
  };

  const rebuildIndex = async (retry: boolean) => {
    if (!rootPath) return;
    // Opening a failed index again retries it without wiping what it has.
    await indexCommand(
      () => (retry ? VaultAPI.explorerIndexOpen(rootPath) : VaultAPI.explorerIndexRebuild(rootPath)),
      rootPath,
      () => true
    );
  };

  const forgetFolder = async (path: string) => {
    const result = await VaultAPI.explorerIndexForget(path);
    if (!result.ok) {
      toast.error("Couldn't delete that folder's index", { message: result.error });
      return;
    }
    forgetRecent(path);
  };

  const chooseFolder = async () => {
    const picked = await open({ directory: true, multiple: false, defaultPath: recentRoots[0]?.root });
    if (typeof picked === 'string') await openRoot(picked);
  };

  // Each column yields before the file view does; the view keeps VIEWER_MIN.
  const room = Math.max(0, rowWidth - VIEWER_MIN);
  const chat = rowWidth ? Math.min(chatWidth, Math.max(CHAT_MIN, room - TREE_MIN)) : chatWidth;
  const tree = rowWidth ? Math.min(treeWidth, Math.max(TREE_MIN, room - chat)) : treeWidth;

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 bg-bg">
      {root ? (
        // The scope bar heads the folder's two columns; the chat column has
        // its own thread bar at the same height beside it.
        <div ref={rowRef} className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
          <div className="flex min-w-0 flex-1 flex-col">
            <ScopeBar
              root={root}
              onClose={() => void closeFolder()}
              indexStatus={indexStatus}
              onRebuildIndex={() => void rebuildIndex(false)}
              onRetryIndex={() => void rebuildIndex(true)}
            />
            <div className="flex min-h-0 flex-1">
              <aside aria-label="Folder tree" className="flex h-full shrink-0 flex-col border-r border-border-subtle bg-surface" style={{ width: tree }}>
                <ExplorerSearch key={root.root} root={root.root} onActiveChange={setSearching} />
                {/* Hidden, not unmounted, so what is open survives a search. */}
                <div className={searching ? 'hidden' : 'min-h-0 flex-1'}>
                  <ExplorerTree key={root.root} root={root.root} />
                </div>
              </aside>
              <SplitHandle
                label="Resize folder tree"
                width={tree}
                min={TREE_MIN}
                max={Math.max(TREE_MIN, room - chat)}
                side={1}
                onResize={setTreeWidth}
                onReset={() => setTreeWidth(TREE_DEFAULT)}
              />
              <main aria-label="File" className="h-full min-w-0 flex-1">
                <ExplorerFileView root={root.root} />
              </main>
            </div>
          </div>
          <SplitHandle
            label="Resize chat"
            width={chat}
            min={CHAT_MIN}
            max={Math.max(CHAT_MIN, room - tree)}
            side={-1}
            onResize={setChatWidth}
            onReset={() => setChatWidth(CHAT_DEFAULT)}
          />
          <section aria-label="Chat about this folder" className="h-full shrink-0 border-l border-border-subtle" style={{ width: chat }}>
            <ExplorerChat threads={threads} />
          </section>
        </div>
      ) : (
        <ExplorerStart
          recentRoots={recentRoots}
          busy={resolving}
          onChoose={() => void chooseFolder()}
          onOpen={(path) => void openRoot(path)}
          onForget={(path) => void forgetFolder(path)}
        />
      )}
    </div>
  );
}

interface ExplorerStartProps {
  recentRoots: ExplorerRoot[];
  busy: boolean;
  onChoose: () => void;
  onOpen: (_path: string) => void;
  onForget: (_path: string) => void;
}

/**
 * Before a folder is open there is nothing to browse: one large choice, and
 * the folders picked before. Once picked, the Explorer stays in that folder
 * until it is closed.
 */
function ExplorerStart({ recentRoots, busy, onChoose, onOpen, onForget }: ExplorerStartProps) {
  return (
    <div className="flex flex-1 items-center justify-center overflow-y-auto px-4 py-10">
      <div className="w-full max-w-lg">
        <FolderTree className="h-6 w-6 text-text-muted" strokeWidth={1.5} aria-hidden="true" />
        <h1 className="mt-4 font-serif text-2xl text-text-primary">Pick the folder to work in.</h1>
        <p className="mt-2 text-sm leading-6 text-text-secondary">
          The Explorer and its chat stay inside the folder you pick. They can read anything below it and nothing above it.
          To work somewhere else, close the folder and pick again. Nothing in the folder is changed.
        </p>
        <p className="mt-2 text-sm leading-6 text-text-secondary">
          Lattice also indexes the folder so its chat can search it by meaning. The index lives in Lattice&apos;s data, not in
          the folder; Forget deletes it.
        </p>
        <button
          type="button"
          disabled={busy}
          onClick={onChoose}
          className="mt-6 flex w-full items-center gap-4 rounded-lg bg-accent px-6 py-6 text-left text-accent-fg transition hover:brightness-95 disabled:opacity-60"
        >
          <FolderOpen className="h-7 w-7 shrink-0" strokeWidth={1.5} aria-hidden="true" />
          <span className="flex flex-col">
            <span className="text-base font-medium">{busy ? 'Opening…' : 'Choose a folder…'}</span>
            <span className="text-[13px] opacity-80">Opens the system folder picker</span>
          </span>
        </button>
        {recentRoots.length > 0 && (
          <section aria-label="Recent folders" className="mt-8">
            <h2 className="text-[11px] font-medium uppercase tracking-[.08em] text-text-muted">Recent</h2>
            <ul className="mt-2 divide-y divide-border-subtle rounded-lg border border-border-subtle">
              {recentRoots.map((item) => (
                <li key={item.root} className="group flex items-center transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.04)]">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => onOpen(item.root)}
                    className="flex min-w-0 flex-1 items-center gap-3 px-3.5 py-2.5 text-left disabled:opacity-60"
                  >
                    <Folder className="h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.6} aria-hidden="true" />
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate text-[13px] text-text-primary">{item.name}</span>
                      <span className="truncate text-[11.5px] text-text-muted">{item.root}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => onForget(item.root)}
                    aria-label={`Forget ${item.name}`}
                    title="Remove from Recent and delete its index"
                    className="mr-2 shrink-0 rounded-md px-2 py-1 text-[12px] text-text-muted opacity-0 transition-opacity duration-fast hover:bg-[hsl(var(--text-primary)/0.05)] hover:text-text-primary focus-visible:opacity-100 group-hover:opacity-100 disabled:opacity-40"
                  >
                    Forget
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
