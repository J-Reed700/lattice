import { createRef, useEffect, type ComponentProps } from 'react';

import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CanvasSurfaceHandle } from './CanvasSurface';

type EditorProps = {
  initialData?: { elements: unknown[]; scrollToContent: boolean };
  theme: string;
  UIOptions: { tools: { image: boolean } };
  excalidrawAPI: (api: unknown) => void;
  onChange: (elements: unknown[], appState: unknown, files: Record<string, unknown>) => void;
  onPaste: (data: unknown) => boolean;
};

const editor = vi.hoisted(() => ({
  props: null as unknown as EditorProps,
  api: { getSceneElements: vi.fn(), getFiles: vi.fn(), getAppState: vi.fn(), updateScene: vi.fn() },
  exportToBlob: vi.fn(),
  exportToSvg: vi.fn(),
}));

vi.mock('@excalidraw/excalidraw', () => ({
  Excalidraw: (props: EditorProps) => {
    editor.props = props;
    useEffect(() => { props.excalidrawAPI(editor.api); }, [props]);
    return <div data-testid="excalidraw">{props.theme}</div>;
  },
  exportToBlob: editor.exportToBlob,
  exportToSvg: editor.exportToSvg,
  serializeAsJSON: (elements: unknown[]) => JSON.stringify({ type: 'excalidraw', version: 2, elements, appState: {}, files: {} }),
}));

const scene = (elements: unknown[] = []) => JSON.stringify({ type: 'excalidraw', version: 2, source: 'test', elements, appState: { viewBackgroundColor: '#fff' }, files: {} });
const rect = { id: 'r1', type: 'rectangle' };
const label = { id: 't1', type: 'text', text: 'Mitochondria' };

type SurfaceProps = ComponentProps<typeof import('./CanvasSurface').default>;

async function renderSurface(overrides: Partial<SurfaceProps> = {}) {
  const { default: CanvasSurface } = await import('./CanvasSurface');
  const ref = createRef<CanvasSurfaceHandle>();
  const props: SurfaceProps = { sceneJson: scene([rect]), theme: 'light', onSceneChange: vi.fn(), onImageBlocked: vi.fn(), onSceneError: vi.fn(), ...overrides };
  const view = render(<CanvasSurface ref={ref} {...props} />);
  return { ref, props, ...view };
}

/** jsdom never fetches stylesheets, so the test plays the browser's part. */
async function finishLoadingStyles(event: 'load' | 'error' = 'load') {
  const link = document.head.querySelector<HTMLLinkElement>('link[data-learning-canvas-styles]');
  expect(link).not.toBeNull();
  await act(async () => { link!.dispatchEvent(new Event(event)); });
}

async function openEditor(overrides: Partial<SurfaceProps> = {}) {
  const view = await renderSurface(overrides);
  if (!screen.queryByTestId('excalidraw')) await finishLoadingStyles();
  await screen.findByTestId('excalidraw');
  return view;
}

beforeEach(() => {
  editor.api.getSceneElements.mockReset().mockReturnValue([rect]);
  editor.api.getFiles.mockReset().mockReturnValue({});
  editor.api.getAppState.mockReset().mockReturnValue({ viewBackgroundColor: '#fff' });
  editor.api.updateScene.mockReset();
  editor.exportToBlob.mockReset();
  editor.exportToSvg.mockReset();
});

describe('CanvasSurface', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows an opening state until the canvas styles load, then the editor without image tools', async () => {
    await renderSurface({ theme: 'dark' });
    expect(screen.getByRole('status')).toHaveTextContent('Opening your canvas…');
    expect(window.EXCALIDRAW_ASSET_PATH).toBe('/excalidraw-assets/');
    const link = document.head.querySelector<HTMLLinkElement>('link[data-learning-canvas-styles]');
    expect(link).toHaveAttribute('href', '/excalidraw-assets/index.css');
    await finishLoadingStyles();
    expect(await screen.findByTestId('excalidraw')).toHaveTextContent('dark');
    expect(link?.dataset.loadState).toBe('loaded');
    expect(editor.props.UIOptions.tools.image).toBe(false);
    expect(editor.props.initialData).toMatchObject({ elements: [rect], scrollToContent: true });
  });

  it('does not scroll to content on an empty canvas', async () => {
    await openEditor({ sceneJson: scene([]) });
    expect(editor.props.initialData?.scrollToContent).toBe(false);
  });

  it.each([
    ['not json', /JSON|Unexpected/],
    [JSON.stringify({ elements: 'nope' }), /drawing data is invalid/],
    [scene([{ id: 'i1', type: 'image' }]), /unsupported element/],
  ])('refuses to open invalid drawing data (%#)', async (sceneJson, message) => {
    await renderSurface({ sceneJson });
    expect(screen.getByRole('alert')).toHaveTextContent(message);
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('reports each drawing change with its outline and live element count', async () => {
    const { props } = await openEditor();
    act(() => editor.props.onChange([rect, label, { id: 'gone', type: 'ellipse', isDeleted: true }], { viewBackgroundColor: '#fff' }, {}));
    expect(props.onSceneChange).toHaveBeenCalledWith({
      sceneJson: expect.stringContaining('Mitochondria'),
      outline: ['Rectangle', 'Mitochondria'],
      elementCount: 2,
    });
  });

  it('removes a dropped-in image element and says images are blocked', async () => {
    const { props } = await openEditor();
    act(() => editor.props.onChange([rect, { id: 'img', type: 'image' }], {}, {}));
    expect(props.onImageBlocked).toHaveBeenCalled();
    expect(editor.api.updateScene).toHaveBeenCalledWith({ elements: [rect] });
    expect(props.onSceneChange).not.toHaveBeenCalled();
  });

  it('refuses attached files and scenes that cannot be saved', async () => {
    const { props } = await openEditor();
    act(() => editor.props.onChange([rect], {}, { f1: {} }));
    expect(props.onSceneError).toHaveBeenLastCalledWith('Image attachments aren’t available on Canvas yet. You can still draw shapes, lines, and text.');
    act(() => editor.props.onChange([{ id: 'e1', type: 'embeddable' }], {}, {}));
    expect(props.onSceneError).toHaveBeenLastCalledWith(expect.stringContaining('External embeds'));
    expect(props.onSceneChange).not.toHaveBeenCalled();
  });

  it('blocks pasted images but lets other pastes through', async () => {
    const { props } = await openEditor();
    expect(editor.props.onPaste({ mixedContent: [{ type: 'imageUrl', value: 'https://example.org/cell.png' }] })).toBe(false);
    expect(props.onImageBlocked).toHaveBeenCalledTimes(1);
    expect(editor.props.onPaste({ mixedContent: [{ type: 'text', value: 'nucleus' }] })).toBe(true);
    expect(props.onImageBlocked).toHaveBeenCalledTimes(1);
  });

  it('stops image files dropped onto the canvas', async () => {
    const { props, container } = await openEditor();
    const surface = container.querySelector('.learning-canvas-surface')!;
    fireEvent.drop(surface, { dataTransfer: { files: [new File(['x'], 'leaf.txt', { type: 'text/plain' })] } });
    expect(props.onImageBlocked).not.toHaveBeenCalled();
    fireEvent.drop(surface, { dataTransfer: { files: [new File(['x'], 'leaf.png', { type: 'image/png' })] } });
    expect(props.onImageBlocked).toHaveBeenCalledTimes(1);
  });

  it('exports the scene as JSON, SVG or PNG', async () => {
    const { ref } = await openEditor();
    const json = await ref.current!.exportScene('json');
    expect(json.type).toBe('application/json');
    expect(JSON.parse(await json.text())).toMatchObject({ source: 'Lattice Learning Studio', elements: [rect] });

    editor.exportToSvg.mockResolvedValue(document.createElementNS('http://www.w3.org/2000/svg', 'svg'));
    const svg = await ref.current!.exportScene('svg');
    expect(svg.type).toBe('image/svg+xml');
    expect(await svg.text()).toContain('<svg');
    expect(editor.exportToSvg).toHaveBeenCalledWith(expect.objectContaining({ elements: [rect], files: {}, exportPadding: 24 }));

    const png = new Blob(['png'], { type: 'image/png' });
    editor.exportToBlob.mockResolvedValue(png);
    await expect(ref.current!.exportScene('png')).resolves.toBe(png);
    expect(editor.exportToBlob).toHaveBeenCalledWith(expect.objectContaining({ mimeType: 'image/png', exportPadding: 24 }));
  });

  it('refuses to export an empty canvas or one holding images', async () => {
    const { ref } = await openEditor();
    editor.api.getSceneElements.mockReturnValue([]);
    await expect(ref.current!.exportScene('png')).rejects.toThrow('Add something to the canvas before exporting.');
    editor.api.getSceneElements.mockReturnValue([rect]);
    editor.api.getFiles.mockReturnValue({ f1: {} });
    await expect(ref.current!.exportScene('json')).rejects.toThrow('Images cannot be exported from this canvas yet.');
    editor.api.getFiles.mockReturnValue({});
    editor.api.getSceneElements.mockReturnValue([{ id: 'img', type: 'image' }]);
    await expect(ref.current!.exportScene('svg')).rejects.toThrow('Images cannot be exported from this canvas yet.');
  });

  it('asks to wait when exporting before the editor has opened', async () => {
    const { ref } = await renderSurface({ sceneJson: 'broken' });
    await expect(ref.current!.exportScene('json')).rejects.toThrow('The drawing surface is still opening. Try again in a moment.');
  });
});

describe('CanvasSurface styles', () => {
  beforeEach(() => {
    vi.resetModules();
    document.head.querySelectorAll('link[data-learning-canvas-styles]').forEach((link) => link.remove());
  });

  it('retries a stylesheet that failed to load last time', async () => {
    const failed = document.createElement('link');
    failed.dataset.learningCanvasStyles = 'true';
    failed.dataset.loadState = 'error';
    document.head.append(failed);
    await renderSurface();
    expect(failed.isConnected).toBe(false);
    const fresh = document.head.querySelector<HTMLLinkElement>('link[data-learning-canvas-styles]');
    expect(fresh?.dataset.loadState).toBe('loading');
    await finishLoadingStyles();
    expect(await screen.findByTestId('excalidraw')).toBeInTheDocument();
  });

  it('opens at once when the stylesheet already loaded', async () => {
    const loaded = document.createElement('link');
    loaded.dataset.learningCanvasStyles = 'true';
    loaded.dataset.loadState = 'loaded';
    document.head.append(loaded);
    await renderSurface();
    expect(await screen.findByTestId('excalidraw')).toBeInTheDocument();
  });
});
