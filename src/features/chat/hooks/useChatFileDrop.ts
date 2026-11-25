import { useCallback, useEffect, useRef, useState } from 'react';

import {
  type StagedFile,
  useStagedFiles,
  useStagedFilesStore,
} from '@/features/chat/stores/stagedFilesStore';

/**
 * Handles files dropped onto the chat panel.
 *
 * Uses Tauri's own webview drag-drop rather than HTML5 `dataTransfer`:
 * `dragDropEnabled` defaults to true in `tauri.conf.json`, so the OS-level
 * handler swallows the DOM events and `dataTransfer` is empty in the packaged
 * app. The import is lazy and guarded so a browser-run test simply gets a hook
 * that never fires.
 */

export type { StagedFile };

export interface ChatFileDropState {
  isDragging: boolean;
  staged: readonly StagedFile[];
  /** Stage paths from somewhere other than a drop (a file dialog, say). */
  add: (_paths: string[]) => void;
  clear: () => void;
  remove: (_path: string) => void;
}

/**
 * `onDrop` hears every dropped path; `stageable` says which of them this hook
 * stages itself (the rest are the caller's, as pictures are).
 */
export function useChatFileDrop(
  panelRef: React.RefObject<HTMLElement | null>,
  onDrop: (_paths: string[]) => void,
  stageable: (_path: string) => boolean = () => true,
  conversationId?: string | null
): ChatFileDropState {
  const [isDragging, setIsDragging] = useState(false);
  const staged = useStagedFiles(conversationId);
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;
  const stageableRef = useRef(stageable);
  stageableRef.current = stageable;
  const conversationIdRef = useRef(conversationId);
  conversationIdRef.current = conversationId;

  const isInsidePanel = useCallback(
    (position?: { x: number; y: number }) => {
      const element = panelRef.current;
      if (!element) return false;
      if (!position) return true;
      const rect = element.getBoundingClientRect();
      // `position` is a PhysicalPosition: device pixels, not CSS pixels.
      const ratio = window.devicePixelRatio || 1;
      const x = position.x / ratio;
      const y = position.y / ratio;
      if (!(x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)) return false;
      // A panel laid over this one, a tangent over the chat, takes the drop.
      const top = document.elementFromPoint?.(x, y);
      return !top || element.contains(top);
    },
    [panelRef]
  );

  const add = useCallback((paths: string[]) => {
    useStagedFilesStore
      .getState()
      .add(conversationIdRef.current, paths, (path) => stageableRef.current(path));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      try {
        const { getCurrentWebview } = await import('@tauri-apps/api/webview');
        const stop = await getCurrentWebview().onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === 'leave') {
            setIsDragging(false);
            return;
          }
          if (payload.type === 'enter' || payload.type === 'over') {
            setIsDragging(isInsidePanel(payload.position));
            return;
          }
          if (payload.type === 'drop') {
            setIsDragging(false);
            if (!isInsidePanel(payload.position)) return;
            const paths = (payload.paths ?? []).filter(Boolean);
            if (paths.length === 0) return;
            add(paths);
            onDropRef.current(paths);
          }
        });
        if (cancelled) stop();
        else unlisten = stop;
      } catch {
        // Not running inside a Tauri webview (tests, plain browser dev).
        // No listener, no drag state — the rest of the panel works unchanged.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [add, isInsidePanel]);

  const clear = useCallback(
    () => useStagedFilesStore.getState().clear(conversationIdRef.current),
    []
  );
  const remove = useCallback(
    (path: string) => useStagedFilesStore.getState().remove(conversationIdRef.current, path),
    []
  );

  return { isDragging, staged, add, clear, remove };
}
