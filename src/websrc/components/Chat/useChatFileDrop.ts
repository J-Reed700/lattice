import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * Files dropped onto the chat panel (BRIEF rank 5, contract §4.5).
 *
 * Uses Tauri's own webview drag-drop rather than HTML5 `dataTransfer`:
 * `dragDropEnabled` defaults to true in `tauri.conf.json`, so the OS-level
 * handler swallows the DOM events and `dataTransfer` is empty in the packaged
 * app. The import is lazy and guarded so a browser-run test simply gets a hook
 * that never fires.
 */

export interface StagedFile {
  path: string;
  name: string;
}

export interface ChatFileDropState {
  isDragging: boolean;
  staged: StagedFile[];
  /** Stage paths from somewhere other than a drop (a file dialog, say). */
  add: (_paths: string[]) => void;
  clear: () => void;
  remove: (_path: string) => void;
}

interface DragDropPayload {
  type: 'enter' | 'over' | 'drop' | 'leave';
  paths?: string[];
  position?: { x: number; y: number };
}

const basename = (path: string): string => {
  const normalized = path.replace(/\\/g, '/');
  const name = normalized.slice(normalized.lastIndexOf('/') + 1);
  return name || path;
};

export function useChatFileDrop(
  panelRef: React.RefObject<HTMLElement | null>,
  onDrop: (_paths: string[]) => void
): ChatFileDropState {
  const [isDragging, setIsDragging] = useState(false);
  const [staged, setStaged] = useState<StagedFile[]>([]);
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;

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
      return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
    },
    [panelRef]
  );

  const add = useCallback((paths: string[]) => {
    const usable = paths.filter(Boolean);
    if (usable.length === 0) return;
    setStaged((current) => {
      const seen = new Set(current.map((file) => file.path));
      const added = usable
        .filter((path) => !seen.has(path))
        .map((path) => ({ path, name: basename(path) }));
      return added.length > 0 ? [...current, ...added] : current;
    });
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      try {
        const { getCurrentWebview } = await import('@tauri-apps/api/webview');
        const stop = await getCurrentWebview().onDragDropEvent((event) => {
          const payload = event.payload as unknown as DragDropPayload;
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
            setStaged((current) => {
              const seen = new Set(current.map((file) => file.path));
              const added = paths
                .filter((path) => !seen.has(path))
                .map((path) => ({ path, name: basename(path) }));
              return added.length > 0 ? [...current, ...added] : current;
            });
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
  }, [isInsidePanel]);

  const clear = useCallback(() => setStaged([]), []);
  const remove = useCallback(
    (path: string) => setStaged((current) => current.filter((file) => file.path !== path)),
    []
  );

  return { isDragging, staged, add, clear, remove };
}
