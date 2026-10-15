/**
 * Where a frozen page is served from, and the small URL helpers the chrome
 * around it needs.
 */

import { convertFileSrc } from '@tauri-apps/api/core';

/** The private scheme the backend registers for frozen pages (design §8). */
export const SNAPSHOT_PROTOCOL = 'lattice-snapshot';

/** The frame address for snapshot `id`; `clean` adds the overlay-hiding layer. */
export function snapshotFrameUrl(id: string, clean: boolean): string {
  return convertFileSrc(`${id}/index.html`, SNAPSHOT_PROTOCOL) + (clean ? '?clean=1' : '');
}

/**
 * The simple normalisation the link bar compares with: no fragment, lower-case
 * host (the URL parser lower-cases it). Null for anything that is not a URL.
 */
export function normalizeSnapshotUrl(raw: string): string | null {
  try {
    const parsed = new URL(raw);
    parsed.hash = '';
    return parsed.href;
  } catch {
    return null;
  }
}

export interface UrlParts {
  host: string;
  path: string;
  secure: boolean;
}

/** Host without `www.`, and the rest (path and query) for the muted half of the pill. */
export function splitUrl(raw: string): UrlParts {
  try {
    const parsed = new URL(raw);
    const rest = `${parsed.pathname}${parsed.search}`;
    return {
      host: parsed.hostname.replace(/^www\./, ''),
      path: rest === '/' ? '' : rest,
      secure: parsed.protocol === 'https:',
    };
  } catch {
    return { host: raw, path: '', secure: false };
  }
}

/** Shorten from the middle, so both the start and the end of a long path stay readable. */
export function middleEllipsis(text: string, max = 48): string {
  if (text.length <= max) return text;
  const head = Math.ceil((max - 1) * 0.55);
  const tail = max - 1 - head;
  return `${text.slice(0, head)}…${text.slice(text.length - tail)}`;
}
