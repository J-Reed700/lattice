import { serializeAsJSON } from '@excalidraw/excalidraw';

import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import type { AppState, BinaryFiles } from '@excalidraw/excalidraw/types';

const MAX_CANVAS_BYTES = 5 * 1024 * 1024;
const MAX_CANVAS_ELEMENTS = 5_000;

export type CanvasScene = {
  type: 'excalidraw';
  version: number;
  source: string;
  elements: ExcalidrawElement[];
  appState: Record<string, unknown>;
  files: Record<string, never>;
};

const PERSISTED_APP_STATE = ['viewBackgroundColor', 'theme', 'gridSize'] as const;

/** Keep Excalidraw's supported element serialization while dropping view and selection state. */
export function serializeCanvasScene(elements: readonly ExcalidrawElement[], appState: Partial<AppState>, files: BinaryFiles): string {
  if (Object.keys(files).length > 0 || elements.some((element) => element.type === 'image')) {
    throw new Error('Images cannot be added to this canvas yet. You can still draw shapes, lines, and text.');
  }
  if (elements.some((element) => element.type === 'embeddable' || element.type === 'iframe')) {
    throw new Error('External embeds aren’t available on Canvas. You can still draw shapes, lines, and text.');
  }
  const liveElements = elements.filter((element) => element.isDeleted !== true);
  if (liveElements.length > MAX_CANVAS_ELEMENTS) throw new Error('This canvas has reached its 5,000-element limit.');
  const official = JSON.parse(serializeAsJSON(elements, appState, {}, 'database')) as CanvasScene;
  const stableState = Object.fromEntries(PERSISTED_APP_STATE.flatMap((key) => appState[key] === undefined ? [] : [[key, appState[key]]]));
  const scene: CanvasScene = {
    type: 'excalidraw',
    version: official.version,
    source: 'Lattice Learning Studio',
    elements: official.elements.filter((element) => !element.isDeleted),
    appState: stableState,
    files: {},
  };
  const serialized = JSON.stringify(scene);
  if (new Blob([serialized]).size > MAX_CANVAS_BYTES) throw new Error('This canvas has reached its 5 MB size limit. Simplify the drawing before saving.');
  return serialized;
}

export function parseCanvasScene(sceneJson: string): CanvasScene {
  const value: unknown = JSON.parse(sceneJson);
  if (!value || typeof value !== 'object' || !('elements' in value) || !Array.isArray(value.elements)) {
    throw new Error('This canvas could not be opened because its drawing data is invalid.');
  }
  const candidate = value as { type?: unknown; elements: unknown[]; version?: unknown; source?: unknown; appState?: unknown; files?: unknown };
  const isRecord = (item: unknown): item is Record<string, unknown> => Boolean(item) && typeof item === 'object' && !Array.isArray(item);
  if (candidate.type !== 'excalidraw' || !Number.isInteger(candidate.version) || Number(candidate.version) < 1
    || !isRecord(candidate.appState) || !isRecord(candidate.files) || Object.keys(candidate.files).length > 0) {
    throw new Error('This canvas could not be opened because its drawing data is invalid.');
  }
  if (candidate.elements.some((element) => !isRecord(element) || typeof element.id !== 'string' || !element.id.trim()
    || typeof element.type !== 'string' || !element.type.trim() || element.type === 'image')) {
    throw new Error('This canvas contains an unsupported element.');
  }
  if (candidate.elements.some((element) => (element as { type: string }).type === 'embeddable' || (element as { type: string }).type === 'iframe')) {
    throw new Error('This canvas contains an external embed, which is not available in Learning Studio.');
  }
  const elements = candidate.elements.filter((element) => (element as { isDeleted?: unknown }).isDeleted !== true) as ExcalidrawElement[];
  return {
    type: 'excalidraw',
    version: candidate.version as number,
    source: typeof candidate.source === 'string' ? candidate.source : 'Lattice Learning Studio',
    elements,
    appState: candidate.appState,
    files: {},
  };
}

export function canvasTextOutline(elements: readonly ExcalidrawElement[]): string[] {
  const labels: string[] = [];
  for (const element of elements) {
    if (element.isDeleted === true) continue;
    if (element.type === 'text') {
      const text = element.text.trim();
      if (text) labels.push(text);
    } else {
      const names: Record<string, string> = {
        rectangle: 'Rectangle', diamond: 'Diamond', ellipse: 'Ellipse', arrow: 'Arrow or connector', line: 'Line', freedraw: 'Freehand stroke', frame: 'Frame', magicframe: 'Frame', embeddable: 'Embedded item', iframe: 'Embedded item', image: 'Image',
      };
      labels.push(names[element.type] ?? 'Drawing element');
    }
  }
  return labels;
}
