import { beforeEach, describe, expect, it, vi } from 'vitest';

import { hasUnsupportedCanvasImage } from '@/features/learning/canvas/canvasPaste';
import { canvasTextOutline, parseCanvasScene, serializeCanvasScene } from '@/features/learning/canvas/canvasScene';

import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import type { AppState } from '@excalidraw/excalidraw/types';


const serializer = vi.hoisted(() => vi.fn());
vi.mock('@excalidraw/excalidraw', () => ({ serializeAsJSON: serializer }));

const rectangle = { id: 'rect-1', type: 'rectangle', isDeleted: false } as unknown as ExcalidrawElement;
const removed = { id: 'deleted-1', type: 'rectangle', isDeleted: true } as unknown as ExcalidrawElement;
const image = { id: 'image-1', type: 'image', isDeleted: false } as unknown as ExcalidrawElement;

beforeEach(() => {
  serializer.mockReset();
  serializer.mockReturnValue(JSON.stringify({ type: 'excalidraw', version: 2, source: 'official', elements: [rectangle, removed], appState: { selectedElementIds: { 'rect-1': true } }, files: {} }));
});

describe('Learning Studio canvas scene adapter', () => {
  it('uses Excalidraw serialization but strips transient state and deleted elements', () => {
    const serialized = serializeCanvasScene([rectangle, removed], { theme: 'dark', viewBackgroundColor: '#fff', selectedElementIds: { 'rect-1': true }, scrollX: 48 } as unknown as Partial<AppState>, {});
    const scene = JSON.parse(serialized);
    expect(serializer).toHaveBeenCalledWith([rectangle, removed], expect.any(Object), {}, 'database');
    expect(scene).toMatchObject({ type: 'excalidraw', source: 'Lattice Learning Studio', elements: [rectangle], appState: { theme: 'dark', viewBackgroundColor: '#fff' }, files: {} });
    expect(scene.appState).not.toHaveProperty('selectedElementIds');
    expect(scene.appState).not.toHaveProperty('scrollX');
  });

  it('rejects embedded images and oversized scenes with actionable messages', () => {
    expect(() => serializeCanvasScene([image], {}, {})).toThrow(/Images cannot be added/);
    expect(() => serializeCanvasScene([{ ...rectangle, type: 'embeddable' } as ExcalidrawElement], {}, {})).toThrow(/External embeds/);
    expect(() => serializeCanvasScene(Array.from({ length: 5_001 }, (_, i) => ({ ...rectangle, id: `shape-${i}` }) as ExcalidrawElement), {}, {})).toThrow(/5,000-element limit/);
    expect(serializer).not.toHaveBeenCalled();
  });

  it('surfaces non-image serialization failures without relabeling them as images', () => {
    serializer.mockImplementation(() => { throw new Error('Scene serializer failed'); });
    expect(() => serializeCanvasScene([rectangle], {}, {})).toThrow('Scene serializer failed');
  });

  it('parses valid scenes defensively and reports invalid data', () => {
    expect(parseCanvasScene(JSON.stringify({ type: 'excalidraw', version: 2, elements: [rectangle, removed], appState: { selectedElementIds: {} }, files: {} }))).toMatchObject({ elements: [rectangle], appState: { selectedElementIds: {} }, files: {} });
    expect(() => parseCanvasScene(JSON.stringify({ type: 'excalidraw', version: 2, elements: [image], appState: {}, files: {} }))).toThrow(/unsupported element/);
    expect(() => parseCanvasScene(JSON.stringify({ type: 'excalidraw', version: 2, elements: [], appState: {}, files: { image: {} } }))).toThrow(/invalid/);
    expect(() => parseCanvasScene(JSON.stringify({ type: 'excalidraw', version: 2, elements: [{ type: 'embeddable', id: 'embed-1' }], appState: {}, files: {} }))).toThrow(/external embed/);
    expect(() => parseCanvasScene('{bad json')).toThrow();
    expect(() => parseCanvasScene(JSON.stringify({ type: 'excalidraw', elements: null }))).toThrow(/drawing data is invalid/);
  });

  it('creates a deterministic text outline from visible element labels', () => {
    const text = { ...rectangle, type: 'text', text: ' A labeled idea ' } as unknown as ExcalidrawElement;
    expect(canvasTextOutline([text, rectangle, removed])).toEqual(['A labeled idea', 'Rectangle']);
  });

  it('blocks image pastes but allows text and vector elements', () => {
    expect(hasUnsupportedCanvasImage({ files: { 'image-1': {} as never } })).toBe(true);
    expect(hasUnsupportedCanvasImage({ elements: [image] })).toBe(true);
    expect(hasUnsupportedCanvasImage({ mixedContent: [{ type: 'imageUrl', value: 'data:image/png;base64,...' }] })).toBe(true);
    expect(hasUnsupportedCanvasImage({ elements: [rectangle], mixedContent: [{ type: 'text', value: 'A label' }] })).toBe(false);
  });
});
