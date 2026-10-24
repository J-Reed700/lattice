import { Suspense, forwardRef, lazy, useCallback, useImperativeHandle, useMemo, useRef } from 'react';

import { hasUnsupportedCanvasImage } from '@/features/learning/canvas/canvasPaste';
import { canvasTextOutline, parseCanvasScene, serializeCanvasScene } from '@/features/learning/canvas/canvasScene';

import type { ExcalidrawElement , Theme } from '@excalidraw/excalidraw/element/types';
import type { AppState, BinaryFiles, ExcalidrawImperativeAPI, ExcalidrawInitialDataState } from '@excalidraw/excalidraw/types';


type ExportKind = 'json' | 'svg' | 'png';
type SceneChange = { sceneJson: string; outline: string[]; elementCount: number };

export interface CanvasSurfaceHandle {
  exportScene: (kind: ExportKind) => Promise<Blob>;
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
  const parsedScene = useMemo(() => {
    try { return { scene: parseCanvasScene(sceneJson), error: null as string | null }; }
    catch (error) { return { scene: null, error: error instanceof Error ? error.message : 'Canvas data is invalid.' }; }
  }, [sceneJson]);
  const initialData = useMemo(() => parsedScene.scene ? {
    elements: parsedScene.scene.elements,
    appState: parsedScene.scene.appState,
    scrollToContent: parsedScene.scene.elements.length > 0,
  } as ExcalidrawInitialDataState : undefined, [parsedScene.scene]);
  const uiOptions = useMemo(() => ({ tools: { image: false }, canvasActions: { clearCanvas: false, export: false, loadScene: false, saveToActiveFile: false, saveAsImage: false, toggleTheme: false } as const }), []);
  const setExcalidrawApi = useCallback((api: ExcalidrawImperativeAPI) => { apiRef.current = api; }, []);
  const renderTopRightUI = useCallback(() => null, []);

  useImperativeHandle(ref, () => ({
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
  }), []);

  const onChange = useCallback((elements: readonly ExcalidrawElement[], appState: AppState, files: BinaryFiles) => {
    const hasImageElement = elements.some((element) => element.type === 'image');
    if (hasImageElement) {
      onImageBlocked();
      apiRef.current?.updateScene({ elements: elements.filter((element) => element.type !== 'image') });
      return;
    }
    if (Object.keys(files).length) {
      onSceneError('Image attachments aren’t available on Canvas yet. You can still draw shapes, lines, and text.');
      return;
    }
    try {
      const nextScene = serializeCanvasScene(elements, appState, {});
      const safeElements = elements.filter((element) => !element.isDeleted);
      onSceneChange({ sceneJson: nextScene, outline: canvasTextOutline(safeElements), elementCount: safeElements.length });
    } catch (error) {
      onSceneError(error instanceof Error ? error.message : 'This drawing could not be saved.');
    }
  }, [onImageBlocked, onSceneChange, onSceneError]);

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
