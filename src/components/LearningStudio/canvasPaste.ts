import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import type { BinaryFiles } from '@excalidraw/excalidraw/types';

export type CanvasClipboardData = {
  files?: BinaryFiles;
  elements?: readonly ExcalidrawElement[];
  mixedContent?: readonly { type: 'text' | 'imageUrl'; value: string }[];
};

/** Embedded assets are disabled until Studio can manage their local lifecycle. */
export function hasUnsupportedCanvasImage(data: CanvasClipboardData): boolean {
  return Boolean(
    Object.keys(data.files ?? {}).length
    || data.elements?.some((element) => element.type === 'image')
    || data.mixedContent?.some((item) => item.type === 'imageUrl'),
  );
}
