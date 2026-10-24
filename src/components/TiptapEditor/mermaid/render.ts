/**
 * Draws ```mermaid fences from chat answers as SVG.
 *
 * The source is model-written, so it is treated as hostile: Mermaid runs with
 * `securityLevel: 'strict'` and SVG-text labels, directives cannot loosen that
 * or swap the app's colours, and the SVG is sanitised again before anyone
 * inserts it. Mermaid and DOMPurify are loaded on first use, so an app that
 * never shows a diagram never downloads them.
 */

import type { MermaidConfig } from 'mermaid';

/** `width` is the drawing's natural width in CSS pixels, when the SVG states one. */
export type RenderResult = { kind: 'svg'; svg: string; width?: number } | { kind: 'error'; message: string };

type MermaidApi = (typeof import('mermaid'))['default'];
type Sanitizer = (typeof import('dompurify'))['default'];

interface Libraries {
  mermaid: MermaidApi;
  purify: Sanitizer;
}

// Mermaid's own defaults, plus the keys that set how labels are drawn and how
// the diagram is coloured. An `%%{init}%%` directive or front matter in the
// source cannot change a key listed here.
const SECURE_KEYS = [
  'secure', 'securityLevel', 'startOnLoad', 'maxTextSize', 'suppressErrorRendering', 'maxEdges',
  'htmlLabels', 'dompurifyConfig', 'theme', 'themeVariables', 'themeCSS', 'darkMode', 'fontFamily',
];

let libraries: Promise<Libraries> | null = null;

function loadLibraries(): Promise<Libraries> {
  libraries ??= Promise.all([import('mermaid'), import('dompurify')]).then(
    ([mermaid, purify]) => ({ mermaid: mermaid.default, purify: purify.default }),
    (error: unknown) => {
      // A chunk that failed to load is fetched again by the next diagram.
      libraries = null;
      throw error;
    },
  );
  return libraries;
}

// Colours from the theme tokens

const HSL_TRIPLE = /^\s*(-?[\d.]+)(?:deg)?\s+([\d.]+)%\s+([\d.]+)%\s*(?:\/\s*([\d.]+)(%?))?\s*$/;

interface Rgba { r: number; g: number; b: number; a: number }

function parseHslTriple(value: string): Rgba | null {
  const match = HSL_TRIPLE.exec(value);
  if (!match) return null;
  const h = ((Number(match[1]) % 360) + 360) % 360;
  const s = Number(match[2]) / 100;
  const l = Number(match[3]) / 100;
  const alpha = match[4] === undefined ? 1 : Number(match[4]) / (match[5] ? 100 : 1);
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const x = chroma * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - chroma / 2;
  const [r, g, b] = h < 60 ? [chroma, x, 0]
    : h < 120 ? [x, chroma, 0]
      : h < 180 ? [0, chroma, x]
        : h < 240 ? [0, x, chroma]
          : h < 300 ? [x, 0, chroma]
            : [chroma, 0, x];
  return { r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255, a: Math.min(1, Math.max(0, alpha)) };
}

function toHex({ r, g, b }: Rgba): string {
  return `#${[r, g, b].map((channel) => Math.round(channel).toString(16).padStart(2, '0')).join('')}`;
}

/**
 * `H S% L%` (optionally `/ alpha`) to `#rrggbb`. Mermaid's theme engine reads
 * only hex colours, so a translucent token (the borders) is laid over
 * `backdrop`, the surface it is drawn on.
 */
export function hslTripleToHex(value: string, backdrop?: string): string | null {
  const color = parseHslTriple(value);
  if (!color) return null;
  if (color.a >= 1) return toHex(color);
  const base = backdrop ? parseHslTriple(backdrop) : null;
  const under = base ?? { r: 255, g: 255, b: 255, a: 1 };
  const mix = (top: number, bottom: number) => top * color.a + bottom * (1 - color.a);
  return toHex({ r: mix(color.r, under.r), g: mix(color.g, under.g), b: mix(color.b, under.b), a: 1 });
}

/** Mermaid's `base` theme derives the rest of its palette from these. */
export function themeVariablesFromTokens(): Record<string, string> {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string) => style.getPropertyValue(`--${name}`).trim();
  const surface = token('surface');
  const hex = (name: string) => hslTripleToHex(token(name), surface);
  const entries: Array<[string, string | null]> = [
    ['background', hex('surface')],
    ['primaryColor', hex('accent-muted')],
    ['primaryBorderColor', hex('border-strong')],
    ['primaryTextColor', hex('text-primary')],
    ['secondaryColor', hex('surface-raised')],
    ['secondaryBorderColor', hex('border-strong')],
    ['tertiaryColor', hex('surface-sunken')],
    ['tertiaryBorderColor', hex('border-default')],
    ['lineColor', hex('text-secondary')],
    ['textColor', hex('text-primary')],
    ['noteBkgColor', hex('surface-raised')],
    ['noteBorderColor', hex('border-strong')],
    ['noteTextColor', hex('text-primary')],
  ];
  // A token that is missing (tests, a stripped stylesheet) leaves Mermaid's default.
  return Object.fromEntries(entries.filter((entry): entry is [string, string] => entry[1] !== null));
}

/**
 * Labels are interface text, not reading text: the UI sans at the app's small
 * size (`text-sm`, 0.875rem), not the answer's serif.
 */
function labelFont(): { family?: string; size: number } {
  const style = getComputedStyle(document.documentElement);
  const family = style.getPropertyValue('--font-sans').trim() || undefined;
  const rootSize = Number.parseFloat(style.fontSize);
  return { family, size: 0.875 * (Number.isFinite(rootSize) && rootSize > 0 ? rootSize : 16) };
}

function mermaidConfig(): MermaidConfig {
  const { family, size } = labelFont();
  const fontFamily = family ? { fontFamily: family } : {};
  return {
    startOnLoad: false,
    securityLevel: 'strict',
    theme: 'base',
    themeVariables: { ...themeVariablesFromTokens(), ...fontFamily, fontSize: `${size}px` },
    ...fontFamily,
    fontSize: size,
    // SVG text, not `foreignObject` HTML. The root key outranks the
    // per-diagram ones (`flowchart.htmlLabels`), which Mermaid deprecates.
    htmlLabels: false,
    // Sequence diagrams set their text from their own keys, not the theme.
    sequence: {
      useMaxWidth: true,
      ...(family ? { actorFontFamily: family, noteFontFamily: family, messageFontFamily: family } : {}),
      actorFontSize: size,
      noteFontSize: size,
      messageFontSize: size,
    },
    suppressErrorRendering: true,
    secure: SECURE_KEYS,
  };
}

// Rendering

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.name === 'UnknownDiagramError') return 'no diagram type was recognised';
  const text = error instanceof Error ? error.message : String(error);
  // Mermaid's parser reports four lines: where, the line, a caret, and every
  // token it would have accepted. Where and what it met say enough.
  const lines = text.split('\n').map((line) => line.trim()).filter(Boolean);
  if (lines.length === 0) return 'unknown error';
  if (lines.length === 1) return lines[0];
  const last = lines[lines.length - 1];
  const met = /got '([^']*)'$/.exec(last);
  return `${lines[0].replace(/:$/, '')}: ${met ? `unexpected ${met[1]}` : last}`;
}

const VIEW_BOX = /^<svg\b[^>]*?\sviewBox="\s*[-\d.e]+[\s,]+[-\d.e]+[\s,]+([\d.e]+)[\s,]+[\d.e]+\s*"/i;

/** The width Mermaid laid the drawing out at: its `viewBox` width. */
function naturalWidth(svg: string): number | undefined {
  const width = Number(VIEW_BOX.exec(svg.trimStart())?.[1]);
  return Number.isFinite(width) && width > 0 ? width : undefined;
}

let renderCount = 0;
// Mermaid keeps one global configuration and draws in a scratch element named
// after the id, so drawings run one at a time.
let queue: Promise<unknown> = Promise.resolve();

async function draw(source: string): Promise<RenderResult> {
  const { mermaid, purify } = await loadLibraries();
  // Every time, not once per theme: the whiteboard's Mermaid import shares
  // this module and sets its own configuration. Reading the tokens here also
  // picks up the theme the caller's epoch stands for.
  mermaid.initialize(mermaidConfig());
  try {
    await mermaid.parse(source);
    renderCount += 1;
    const { svg } = await mermaid.render(`lattice-mermaid-${renderCount}`, source);
    const clean = purify.sanitize(svg, {
      USE_PROFILES: { svg: true, svgFilters: true },
      ADD_TAGS: ['style'],
      FORBID_TAGS: ['script', 'foreignObject'],
      FORBID_ATTR: ['onload', 'onclick'],
    });
    return { kind: 'svg', svg: clean, width: naturalWidth(clean) };
  } catch (error) {
    return { kind: 'error', message: errorMessage(error) };
  }
}

interface CacheEntry {
  promise: Promise<RenderResult>;
  result?: RenderResult;
}

// A memory bound: each entry holds a drawn SVG string. Streaming re-mounts and
// a theme switch back and forth stay well inside it.
const CACHE_LIMIT = 200;
const cache = new Map<string, CacheEntry>();

function cacheKey(source: string, themeEpoch: number): string {
  return `${themeEpoch}\n${source}`;
}

/** The finished drawing for this source and theme, if one is ready now. */
export function peekMermaid(source: string, themeEpoch: number): RenderResult | undefined {
  return cache.get(cacheKey(source, themeEpoch))?.result;
}

/**
 * Draws `source` with the current theme. The same source and theme are drawn
 * once; later calls share the first call's result.
 */
export function renderMermaid(source: string, themeEpoch: number): Promise<RenderResult> {
  const key = cacheKey(source, themeEpoch);
  const cached = cache.get(key);
  if (cached) return cached.promise;
  const promise = queue.then(() => draw(source));
  queue = promise.catch(() => undefined);
  const entry: CacheEntry = { promise };
  void promise.then(
    (result) => { entry.result = result; },
    // Loading Mermaid failed (a missing chunk). Forget the entry so a later
    // mount tries again.
    () => { if (cache.get(key) === entry) cache.delete(key); },
  );
  cache.set(key, entry);
  if (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  return promise;
}
