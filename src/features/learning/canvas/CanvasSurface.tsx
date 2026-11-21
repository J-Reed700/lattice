import { Suspense, forwardRef, lazy, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';

import { hasUnsupportedCanvasImage } from '@/features/learning/canvas/canvasPaste';
import { canvasTextOutline, parseCanvasScene, serializeCanvasScene } from '@/features/learning/canvas/canvasScene';

import type { ExcalidrawElement , Theme } from '@excalidraw/excalidraw/element/types';
import type { AppState, BinaryFiles, ExcalidrawImperativeAPI, ExcalidrawInitialDataState } from '@excalidraw/excalidraw/types';


type ExportKind = 'json' | 'svg' | 'png';
type SceneChange = { sceneJson: string; outline: string[]; elementCount: number };

const SERIALIZE_DEBOUNCE_MS = 250;

type PersistedAppStateSnapshot = {
  viewBackgroundColor: unknown;
  theme: unknown;
  gridSize: unknown;
};

type ElementSnapshot = {
  ref: ExcalidrawElement;
  id: string;
  version: number | undefined;
  versionNonce: number | undefined;
  isDeleted: boolean | undefined;
};

function captureElementSnapshots(elements: readonly ExcalidrawElement[]): ElementSnapshot[] {
  return elements.map((element) => ({
    ref: element,
    id: element.id,
    version: element.version,
    versionNonce: element.versionNonce,
    isDeleted: element.isDeleted,
  }));
}

function capturePersistedAppState(appState: Partial<AppState> | Record<string, unknown>): PersistedAppStateSnapshot {
  return {
    viewBackgroundColor: appState.viewBackgroundColor,
    theme: appState.theme,
    gridSize: appState.gridSize,
  };
}

function areElementsUnchanged(
  prev: readonly ElementSnapshot[] | null,
  next: readonly ExcalidrawElement[],
): boolean {
  if (!prev) return false;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < next.length; i += 1) {
    const a = prev[i];
    const b = next[i];
    // Excalidraw's mutateElement() updates elements in place (same object ref,
    // new version/versionNonce/isDeleted), so version metadata must be checked
    // before falling back to reference equality.
    if (a.version !== undefined && b.version !== undefined) {
      if (
        a.id === b.id &&
        a.version === b.version &&
        a.versionNonce === b.versionNonce &&
        a.isDeleted === b.isDeleted
      ) {
        continue;
      }
      return false;
    }
    if (a.ref === b) continue;
    return false;
  }
  return true;
}

function isPersistedAppStateUnchanged(
  prev: PersistedAppStateSnapshot | null,
  next: AppState,
): boolean {
  if (!prev) return false;
  return (
    prev.viewBackgroundColor === next.viewBackgroundColor &&
    prev.theme === next.theme &&
    prev.gridSize === next.gridSize
  );
}

export interface CanvasSurfaceHandle {
  exportScene: (kind: ExportKind) => Promise<Blob>;
  flush?: () => SceneChange | null;
}

type Props = {
  sceneJson: string;
  theme: Theme;
  onSceneChange: (scene: SceneChange) => void;
  onImageBlocked: () => void;
  onSceneError: (message: string) => void;
};

declare global {
  interface Window { EXCALIDRAW_ASSET_PATH?: string }
}

const loadEditor = async () => {
  let stylesReady: Promise<void> = Promise.resolve();
  if (typeof window !== 'undefined') {
    window.EXCALIDRAW_ASSET_PATH = '/excalidraw-assets/';
    let stylesheet = document.querySelector<HTMLLinkElement>('link[data-learning-canvas-styles]');
    if (stylesheet?.dataset.loadState === 'error') { stylesheet.remove(); stylesheet = null; }
    if (!stylesheet) {
      stylesheet = document.createElement('link');
      stylesheet.rel = 'stylesheet';
      stylesheet.href = '/excalidraw-assets/index.css';
      stylesheet.dataset.learningCanvasStyles = 'true';
      stylesheet.dataset.loadState = 'loading';
      stylesheet.addEventListener('load', () => { stylesheet!.dataset.loadState = 'loaded'; }, { once: true });
      stylesheet.addEventListener('error', () => { stylesheet!.dataset.loadState = 'error'; }, { once: true });
      document.head.append(stylesheet);
    }
    if (stylesheet.dataset.loadState === 'loading' || (!stylesheet.sheet && stylesheet.dataset.loadState !== 'loaded')) stylesReady = new Promise<void>((resolve, reject) => {
      stylesheet?.addEventListener('load', () => resolve(), { once: true });
      stylesheet?.addEventListener('error', () => reject(new Error('Canvas styles could not be loaded.')), { once: true });
    });
  }
  const [module] = await Promise.all([import('@excalidraw/excalidraw'), stylesReady]);
  return { default: module.Excalidraw };
};
const LazyExcalidraw = lazy(loadEditor);

const CanvasSurface = forwardRef<CanvasSurfaceHandle, Props>(({ sceneJson, theme, onSceneChange, onImageBlocked, onSceneError }, ref) => {
  const apiRef = useRef<ExcalidrawImperativeAPI | null>(null);
  const onSceneChangeRef = useRef(onSceneChange);
  onSceneChangeRef.current = onSceneChange;
  const onSceneErrorRef = useRef(onSceneError);
  onSceneErrorRef.current = onSceneError;

  // Parse initialData only once per surface mount (`CanvasPanel` keys this
  // component by `${selected.id}:${surfaceVersion}` for external resets).
  const [parsedScene] = useState(() => {
    try { return { scene: parseCanvasScene(sceneJson), error: null as string | null }; }
    catch (error) { return { scene: null, error: error instanceof Error ? error.message : 'Canvas data is invalid.' }; }
  });
  const initialData = useMemo(() => parsedScene.scene ? {
    elements: parsedScene.scene.elements,
    appState: parsedScene.scene.appState,
    scrollToContent: parsedScene.scene.elements.length > 0,
  } as ExcalidrawInitialDataState : undefined, [parsedScene.scene]);

  const lastElementsRef = useRef<ElementSnapshot[] | null>(
    parsedScene.scene ? captureElementSnapshots(parsedScene.scene.elements) : null,
  );
  const lastAppStateRef = useRef<PersistedAppStateSnapshot | null>(
    parsedScene.scene ? capturePersistedAppState(parsedScene.scene.appState) : null,
  );
  const pendingSnapshotRef = useRef<{ elements: readonly ExcalidrawElement[]; appState: AppState } | null>(null);
  const debounceTimerRef = useRef<number | null>(null);

  const flushPending = useCallback((): SceneChange | null => {
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    const pending = pendingSnapshotRef.current;
    if (!pending) return null;
    pendingSnapshotRef.current = null;
    try {
      const nextScene = serializeCanvasScene(pending.elements, pending.appState, {});
      const safeElements = pending.elements.filter((element) => !element.isDeleted);
      const change: SceneChange = {
        sceneJson: nextScene,
        outline: canvasTextOutline(safeElements),
        elementCount: safeElements.length,
      };
      onSceneChangeRef.current(change);
      return change;
    } catch (error) {
      onSceneErrorRef.current(error instanceof Error ? error.message : 'This drawing could not be saved.');
      return null;
    }
  }, []);

  // Cancel any pending debounce on unmount without invoking onSceneChange:
  // CanvasPanel calls ref.flush() explicitly before save/switch/checkpoint and on unmount,
  // whereas reload/restore/template resets bump surfaceVersion to discard local edits.
  useEffect(() => () => {
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    pendingSnapshotRef.current = null;
  }, []);

  const uiOptions = useMemo(() => ({ tools: { image: false }, canvasActions: { clearCanvas: false, export: false, loadScene: false, saveToActiveFile: false, saveAsImage: false, toggleTheme: false } as const }), []);
  const setExcalidrawApi = useCallback((api: ExcalidrawImperativeAPI) => { apiRef.current = api; }, []);
  const renderTopRightUI = useCallback(() => null, []);

  useImperativeHandle(ref, () => ({
    flush: flushPending,
    async exportScene(kind) {
      const { exportToBlob, exportToSvg } = await import('@excalidraw/excalidraw');
      const api = apiRef.current;
      if (!api) throw new Error('The drawing surface is still opening. Try again in a moment.');
      const elements = api.getSceneElements();
      if (!elements.length) throw new Error('Add something to the canvas before exporting.');
      const files = api.getFiles();
      if (Object.keys(files).length || elements.some((element) => element.type === 'image')) throw new Error('Images cannot be exported from this canvas yet.');
      const state = api.getAppState();
      if (kind === 'json') return new Blob([serializeCanvasScene(elements, state, {})], { type: 'application/json' });
      if (kind === 'svg') {
        const svg = await exportToSvg({ elements, appState: state, files: {}, exportPadding: 24 });
        return new Blob([new XMLSerializer().serializeToString(svg)], { type: 'image/svg+xml' });
      }
      return exportToBlob({ elements, appState: state, files: {}, mimeType: 'image/png', exportPadding: 24 });
    },
  }), [flushPending]);

  const onChange = useCallback((elements: readonly ExcalidrawElement[], appState: AppState, files: BinaryFiles) => {
    const hasImageElement = elements.some((element) => element.type === 'image');
    if (hasImageElement) {
      onImageBlocked();
      apiRef.current?.updateScene({ elements: elements.filter((element) => element.type !== 'image') });
      return;
    }
    if (Object.keys(files).length) {
      onSceneErrorRef.current('Image attachments aren’t available on Canvas yet. You can still draw shapes, lines, and text.');
      return;
    }
    if (
      areElementsUnchanged(lastElementsRef.current, elements) &&
      isPersistedAppStateUnchanged(lastAppStateRef.current, appState)
    ) {
      return;
    }
    lastElementsRef.current = captureElementSnapshots(elements);
    lastAppStateRef.current = capturePersistedAppState(appState);
    pendingSnapshotRef.current = { elements, appState };
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
    }
    debounceTimerRef.current = window.setTimeout(() => {
      debounceTimerRef.current = null;
      flushPending();
    }, SERIALIZE_DEBOUNCE_MS);
  }, [flushPending, onImageBlocked]);

  const handlePaste = useCallback((data: { files?: BinaryFiles; elements?: readonly ExcalidrawElement[]; mixedContent?: { type: 'text' | 'imageUrl'; value: string }[] }) => {
    const hasImage = hasUnsupportedCanvasImage(data);
    if (hasImage) onImageBlocked();
    return !hasImage;
  }, [onImageBlocked]);

  if (parsedScene.error || !parsedScene.scene) return <div role="alert" className="grid h-full min-h-[460px] place-items-center bg-surface p-6 text-sm text-rose-700">{parsedScene.error || 'This canvas could not be opened.'}</div>;

  return <div className="learning-canvas-surface relative isolate h-[min(72vh,720px)] min-h-[460px] overflow-hidden rounded-xl border border-border" onDropCapture={(event) => {
    if ([...event.dataTransfer.files].some((file) => file.type.startsWith('image/'))) { event.preventDefault(); event.stopPropagation(); onImageBlocked(); }
  }}>
    <style>{'.learning-canvas-surface .collab-button,.learning-canvas-surface .main-menu-trigger,.learning-canvas-surface .library-button,.learning-canvas-surface .ai-button{display:none!important}.learning-canvas-surface .excalidraw{--color-primary:var(--accent)}'}</style>
    <Suspense fallback={<div role="status" className="grid h-full min-h-[460px] place-items-center bg-[#f5f1ea] text-sm text-text-muted dark:bg-[#20201f]">Opening your canvas…</div>}>
      <LazyExcalidraw initialData={initialData} theme={theme} name="Learning Studio Canvas" excalidrawAPI={setExcalidrawApi} onChange={onChange} onPaste={handlePaste} validateEmbeddable={false} aiEnabled={false} UIOptions={uiOptions} renderTopRightUI={renderTopRightUI} />
    </Suspense>
  </div>;
});

export default CanvasSurface;
