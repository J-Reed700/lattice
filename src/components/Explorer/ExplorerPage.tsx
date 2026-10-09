import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';

import {
  explorerFoldersQueryOptions,
  resolveExplorerRoot,
  useExplorerFolderMutations,
  useExplorerIndexCommands,
  useExplorerIndexStatus,
  useExplorerIndexStatusEvents,
} from '@/features/explorer/api/queries';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore } from '@/stores/explorerStore';
import { toast } from '@/stores/toastStore';

import { ExplorerChat } from './ExplorerChat';
import { ExplorerFileView } from './ExplorerFileView';
import { ExplorerSearch } from './ExplorerSearch';
import { ExplorerStart } from './ExplorerStart';
import { ExplorerTree } from './ExplorerTree';
import { FolderSettingsDialog, type FolderSettingsTarget } from './FolderSettingsDialog';
import { ScopeBar } from './ScopeBar';
import { SplitHandle } from './SplitHandle';
import { useExplorerThread } from './useExplorerThread';

import './explorer.css';

const TREE_WIDTH_KEY = 'explorer.treeWidth';
const TREE_HIDDEN_KEY = 'explorer.treeHidden';
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

function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? '1' : '0');
  } catch {
    // Preference only.
  }
}

/**
 * Explorer: a folder from disk beside a chat.
 *
 * Tree | file | chat. The chat is the ordinary one, on a thread bound to the
 * folder; it can read the folder, and its line references light up the file.
 */
export function ExplorerPage() {
  const root = useExplorerStore((state) => state.root);
  const setRoot = useExplorerStore((state) => state.setRoot);
  const queryClient = useQueryClient();
  const index = useExplorerIndexCommands();
  const { setSettings } = useExplorerFolderMutations();
  const loadSpaces = useConversationsStore((state) => state.loadSpaces);
  const { fetchDownloadedModels } = useDownloadedModels();
  /** The folder being resolved before it opens. */
  const [opening, setOpening] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  const [searching, setSearching] = useState(false);
  const [settingsFor, setSettingsFor] = useState<FolderSettingsTarget | null>(null);
  const [treeWidth, setTreeWidth] = useState(() => readWidth(TREE_WIDTH_KEY, TREE_DEFAULT));
  const [chatWidth, setChatWidth] = useState(() => readWidth(CHAT_WIDTH_KEY, CHAT_DEFAULT));
  const [treeHidden, setTreeHidden] = useState(() => readFlag(TREE_HIDDEN_KEY));
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
    void index.open(rootPath, () => current);
    return () => {
      current = false;
    };
  }, [index, rootPath]);

  // Progress arrives as events, for the scope bar, the chat and the folders list.
  useExplorerIndexStatusEvents();
  const indexStatus = useExplorerIndexStatus(rootPath);

  useEffect(() => writeWidth(TREE_WIDTH_KEY, treeWidth), [treeWidth]);
  useEffect(() => writeWidth(CHAT_WIDTH_KEY, chatWidth), [chatWidth]);
  useEffect(() => writeFlag(TREE_HIDDEN_KEY, treeHidden), [treeHidden]);

  // ⌘\ hides and shows the folder tree, as it does Chat's sidebar.
  useEffect(() => {
    if (!rootPath) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && event.key === '\\') {
        event.preventDefault();
        setTreeHidden((hidden) => !hidden);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [rootPath]);

  useEffect(() => {
    const element = rowRef.current;
    if (!element || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setRowWidth(element.clientWidth));
    observer.observe(element);
    setRowWidth(element.clientWidth);
    return () => observer.disconnect();
  }, [root]);

  /** Opens `path`; a folder from the list keeps the name it was given there. */
  const openRoot = async (path: string, name?: string) => {
    setOpening(path);
    try {
      const result = await resolveExplorerRoot(path);
      if (!result.ok) {
        toast.error("Couldn't open that folder", { message: result.error });
        return;
      }
      setRoot({ ...result.data, name: name ?? result.data.name });
    } finally {
      setOpening(null);
    }
  };

  // Closing waits for the batch in flight and a save, so it says so.
  const closeFolder = async () => {
    setClosing(true);
    try {
      await index.close(rootPath);
      setRoot(null);
    } finally {
      setClosing(false);
    }
  };

  const rebuildIndex = async (retry: boolean) => {
    if (!rootPath) return;
    // Opening a failed index again retries it without wiping what it has.
    await index.rebuild(rootPath, retry);
  };

  /** The open folder's row, read fresh: its settings may have changed on the start screen. */
  const openSettings = async () => {
    if (!rootPath) return;
    let folder: FolderSettingsTarget | undefined;
    try {
      const list = await queryClient.fetchQuery({ ...explorerFoldersQueryOptions(), staleTime: 0 });
      folder = list.folders.find((item) => item.root === rootPath);
      if (!folder) throw new Error('It isn’t in Your folders yet. Try again in a moment.');
    } catch (error) {
      toast.error("Couldn't load this folder's settings", {
        message: error instanceof Error ? error.message : String(error),
      });
      return;
    }
    setSettingsFor(folder);
  };

  const saveSettings = async (target: FolderSettingsTarget, instructions: string, spaceId: string) => {
    try {
      await setSettings.mutateAsync({ root: target.root, instructions, spaceId });
      return true;
    } catch (error) {
      toast.error("Couldn't save this folder's settings", {
        message: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  };

  const chooseFolder = async (defaultPath?: string) => {
    const picked = await open({ directory: true, multiple: false, defaultPath });
    if (typeof picked === 'string') await openRoot(picked);
  };

  // Each column yields before the file view does; the view keeps VIEWER_MIN.
  // A hidden tree takes no room, so the chat can have what it would have had.
  const room = Math.max(0, rowWidth - VIEWER_MIN);
  const treeFloor = treeHidden ? 0 : TREE_MIN;
  const chat = rowWidth ? Math.min(chatWidth, Math.max(CHAT_MIN, room - treeFloor)) : chatWidth;
  const tree = treeHidden ? 0 : rowWidth ? Math.min(treeWidth, Math.max(TREE_MIN, room - chat)) : treeWidth;

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
              closing={closing}
              indexStatus={indexStatus}
              onRebuildIndex={() => void rebuildIndex(false)}
              onRetryIndex={() => void rebuildIndex(true)}
              onSettings={() => void openSettings()}
              treeHidden={treeHidden}
              onToggleTree={() => setTreeHidden((hidden) => !hidden)}
            />
            <div className="flex min-h-0 flex-1">
              {/* Hidden, not unmounted, so the tree keeps its place and a search its results. */}
              <aside
                aria-label="Folder tree"
                className={treeHidden ? 'hidden' : 'flex h-full shrink-0 flex-col border-r border-border-subtle bg-surface'}
                style={{ width: tree }}
              >
                <ExplorerSearch key={root.root} root={root.root} onActiveChange={setSearching} />
                {/* Hidden, not unmounted, so what is open survives a search. */}
                <div className={searching ? 'hidden' : 'min-h-0 flex-1'}>
                  <ExplorerTree key={root.root} root={root.root} />
                </div>
              </aside>
              {!treeHidden && (
                <SplitHandle
                  label="Resize folder tree"
                  width={tree}
                  min={TREE_MIN}
                  max={Math.max(TREE_MIN, room - chat)}
                  side={1}
                  onResize={setTreeWidth}
                  onReset={() => setTreeWidth(TREE_DEFAULT)}
                />
              )}
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
          {settingsFor && (
            <FolderSettingsDialog
              folder={settingsFor}
              onCancel={() => setSettingsFor(null)}
              onSave={(instructions, spaceId) => saveSettings(settingsFor, instructions, spaceId)}
            />
          )}
        </div>
      ) : (
        <ExplorerStart
          opening={opening}
          onChoose={(defaultPath) => void chooseFolder(defaultPath)}
          onOpen={(folder) => void openRoot(folder.root, folder.name)}
        />
      )}
    </div>
  );
}
