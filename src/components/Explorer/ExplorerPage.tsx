import { useEffect, useRef, useState } from 'react';

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { useDownloadedModels } from '@/hooks/useDownloadedModels';
import { VaultAPI } from '@/lib/api';
import { queryClient } from '@/lib/queryClient';
import { useConversationsStore } from '@/stores/conversationsStore';
import { useExplorerStore, type FolderIndexStatus } from '@/stores/explorerStore';
import { toast } from '@/stores/toastStore';
import type { ApiResult } from '@/types';

import { ExplorerChat } from './ExplorerChat';
import { ExplorerFileView } from './ExplorerFileView';
import { ExplorerSearch } from './ExplorerSearch';
import { ExplorerStart } from './ExplorerStart';
import { ExplorerTree } from './ExplorerTree';
import { FolderSettingsDialog, type FolderSettingsTarget } from './FolderSettingsDialog';
import { INDEX_STATUS_EVENT } from './indexProgress';
import { ScopeBar } from './ScopeBar';
import { SplitHandle } from './SplitHandle';
import { useExplorerThread } from './useExplorerThread';

import './explorer.css';

export { INDEX_STATUS_EVENT } from './indexProgress';

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
  return {
    root,
    indexRoot: root,
    state: 'error',
    filesTotal: 0,
    filesIndexed: 0,
    passagesTotal: 0,
    passagesEmbedded: 0,
    passagesPerSecond: null,
    etaSeconds: null,
    message,
  };
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
  const setRoot = useExplorerStore((state) => state.setRoot);
  const indexStatus = useExplorerStore((state) => state.indexStatus);
  const setIndexStatus = useExplorerStore((state) => state.setIndexStatus);
  const loadSpaces = useConversationsStore((state) => state.loadSpaces);
  const { fetchDownloadedModels } = useDownloadedModels();
  /** The folder being resolved before it opens. */
  const [opening, setOpening] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  const [searching, setSearching] = useState(false);
  const [settingsFor, setSettingsFor] = useState<FolderSettingsTarget | null>(null);
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

  /** Opens `path`; a folder from the list keeps the name it was given there. */
  const openRoot = async (path: string, name?: string) => {
    setOpening(path);
    try {
      const result = await VaultAPI.explorerResolveRoot(path);
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
      await VaultAPI.explorerIndexClose();
      setRoot(null);
    } finally {
      setClosing(false);
    }
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

  /** The open folder's row, read fresh: its settings may have changed on the start screen. */
  const openSettings = async () => {
    if (!rootPath) return;
    const result = await VaultAPI.explorerFoldersList();
    const folder = result.ok ? result.data.folders.find((item) => item.root === rootPath) : undefined;
    if (!folder) {
      toast.error("Couldn't load this folder's settings", {
        message: result.ok ? 'It isn’t in Your folders yet. Try again in a moment.' : result.error,
      });
      return;
    }
    setSettingsFor(folder);
  };

  const saveSettings = async (target: FolderSettingsTarget, instructions: string, spaceId: string) => {
    const result = await VaultAPI.explorerFolderSetSettings(target.root, instructions, spaceId);
    if (!result.ok) {
      toast.error("Couldn't save this folder's settings", { message: result.error });
      return false;
    }
    // Moved threads change space in the Chat sidebar and the thread bar.
    if (result.data > 0) void queryClient.invalidateQueries({ queryKey: conversationKeys.all });
    return true;
  };

  const chooseFolder = async (defaultPath?: string) => {
    const picked = await open({ directory: true, multiple: false, defaultPath });
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
              closing={closing}
              indexStatus={indexStatus}
              onRebuildIndex={() => void rebuildIndex(false)}
              onRetryIndex={() => void rebuildIndex(true)}
              onSettings={() => void openSettings()}
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
