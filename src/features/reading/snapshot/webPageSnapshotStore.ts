/**
 * How the person likes to look at cited web pages, for this session.
 *
 * `mode` is remembered only while the app runs: null until they pick one, and
 * then their pick wins over the "Page when a frozen copy exists" default.
 * `zoom` and `clean` are preferences and survive a restart.
 */

import { create } from 'zustand';

export type WebPageMode = 'reader' | 'page';

export const ZOOM_KEY = 'chat.reader.pageZoom';
export const CLEAN_KEY = 'chat.reader.pageClean';
export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 2;
export const ZOOM_STEP = 0.1;

export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return 1;
  return Math.round(Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom)) * 10) / 10;
}

function readStored(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch {
    // Preference only.
  }
}

function readZoom(): number {
  const raw = readStored(ZOOM_KEY);
  return raw === null ? 1 : clampZoom(Number(raw));
}

function readClean(): boolean {
  return readStored(CLEAN_KEY) !== 'false';
}

export interface WebPageSnapshotState {
  mode: WebPageMode | null;
  zoom: number;
  clean: boolean;
  setMode: (_mode: WebPageMode) => void;
  setZoom: (_zoom: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;
  setClean: (_clean: boolean) => void;
}

export const useWebPageSnapshotStore = create<WebPageSnapshotState>((set, get) => ({
  mode: null,
  zoom: readZoom(),
  clean: readClean(),

  setMode: (mode) => set({ mode }),

  setZoom: (value) => {
    const zoom = clampZoom(value);
    if (zoom === get().zoom) return;
    writeStored(ZOOM_KEY, String(zoom));
    set({ zoom });
  },
  zoomIn: () => get().setZoom(get().zoom + ZOOM_STEP),
  zoomOut: () => get().setZoom(get().zoom - ZOOM_STEP),
  resetZoom: () => get().setZoom(1),

  setClean: (clean) => {
    writeStored(CLEAN_KEY, String(clean));
    set({ clean });
  },
}));
