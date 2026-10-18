/**
 * What a model's vision encoder does with a picture: the size it is sent at
 * and what that costs in prompt tokens. A port of the backend's
 * `domain/models/vision.rs`, for the Settings cost preview and the model
 * picker. The two must agree: `visionGeometry.test.ts` carries the vectors of
 * the Rust tests.
 *
 * A geometry is declared per model (catalog, Settings or a cloud provider's
 * default), never measured from a server.
 */

import type { CloudProvider, VisionDeclarationDto, VisionGeometry } from '../lib/bindings';

export type { VisionGeometry };
export type VisionDeclaration = VisionDeclarationDto;

/** The backend's `VisionPreset`, in its serde names. No command carries it, so bindings omit it. */
export type VisionPreset = 'qwen3_vl' | 'qwen25_vl' | 'gemma_siglip' | 'open_ai' | 'anthropic' | 'gemini';

export interface FittedPicture {
  width: number;
  height: number;
  tokens: number;
}

const U32_MAX = 0xffff_ffff;
const sat = (value: number) => Math.min(U32_MAX, value);

/**
 * For a model whose geometry nobody declared: pictures go no larger than
 * 1,024 squares of 28 px and are charged as much, which is at or above what
 * the local families and OpenAI charge for the same picture.
 */
export const CONSERVATIVE_DEFAULT: VisionGeometry = {
  kind: 'grid',
  token_side: 28,
  min_pixels: 4 * 28 * 28,
  max_pixels: 1024 * 28 * 28,
  overhead_tokens: 2,
};

/** The named geometries Settings offers, in the backend's `VisionPreset::ALL` order. */
export const VISION_PRESETS: ReadonlyArray<{ id: VisionPreset; label: string; geometry: VisionGeometry }> = [
  {
    id: 'qwen3_vl',
    label: 'Qwen 3.5–3.8 VL',
    geometry: { kind: 'grid', token_side: 32, min_pixels: 64 * 32 * 32, max_pixels: 2048 * 32 * 32, overhead_tokens: 2 },
  },
  {
    id: 'qwen25_vl',
    label: 'Qwen 2.5 VL',
    geometry: { kind: 'grid', token_side: 28, min_pixels: 4 * 28 * 28, max_pixels: 1280 * 28 * 28, overhead_tokens: 2 },
  },
  { id: 'gemma_siglip', label: 'Gemma 3/4', geometry: { kind: 'fixed', side: 896, tokens: 256 } },
  {
    id: 'open_ai',
    label: 'OpenAI',
    geometry: { kind: 'tiles', tile_side: 512, tokens_per_tile: 170, base_tokens: 85, max_tiles: 16, max_side: 2048 },
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    geometry: { kind: 'grid', token_side: 27, min_pixels: 0, max_pixels: 1568 * 1568, overhead_tokens: 0 },
  },
  {
    id: 'gemini',
    label: 'Gemini',
    geometry: { kind: 'tiles', tile_side: 768, tokens_per_tile: 258, base_tokens: 0, max_tiles: 6, max_side: 3072 },
  },
];

export function presetGeometry(preset: VisionPreset): VisionGeometry {
  return VISION_PRESETS.find((p) => p.id === preset)!.geometry;
}

export function sameGeometry(a: VisionGeometry, b: VisionGeometry): boolean {
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((key) => left[key] === right[key]);
}

/** The preset a geometry is exactly, or `null` for a custom one. */
export function presetOf(geometry: VisionGeometry): VisionPreset | null {
  return VISION_PRESETS.find((p) => sameGeometry(p.geometry, geometry))?.id ?? null;
}

/**
 * The provider's published geometry for a cloud model whose family the
 * provider documents as taking pictures; `null` leaves it unable to see until
 * the user declares one. Mirrors `cloud_vision_default`.
 */
export function cloudVisionDefault(provider: CloudProvider, model: string): VisionGeometry | null {
  const name = model.trim().toLowerCase();
  switch (provider) {
    case 'openai':
      return ['gpt-4o', 'gpt-4.1', 'gpt-5', 'o3', 'o4'].some((family) => name.startsWith(family))
        ? presetGeometry('open_ai') : null;
    case 'anthropic':
      return name.startsWith('claude-') ? presetGeometry('anthropic') : null;
    case 'google':
      return name.startsWith('gemini-') ? presetGeometry('gemini') : null;
  }
}

/** Why a declared geometry cannot be used, as the settings check words it. */
export function geometryProblem(geometry: VisionGeometry): string | null {
  switch (geometry.kind) {
    case 'grid':
      if (geometry.token_side === 0) return 'a grid geometry needs a token side above 0';
      if (geometry.min_pixels > geometry.max_pixels) return "a grid geometry's minimum area cannot exceed its maximum";
      return null;
    case 'fixed':
      return geometry.side === 0 ? 'a fixed geometry needs a side above 0' : null;
    case 'tiles':
      return geometry.tile_side === 0 ? 'a tiled geometry needs a tile side above 0' : null;
  }
}

/** The fewest grid squares a `grid` picture is charged: the minimum area, never less than one. */
function minSquares(geometry: Extract<VisionGeometry, { kind: 'grid' }>): number {
  const square = Math.max(1, geometry.token_side) ** 2;
  return Math.max(1, sat(Math.ceil(geometry.min_pixels / square)));
}

/**
 * Area at most `maxArea`, then each side to the nearest multiple of
 * `tokenSide` (at least one square, never above the source). When rounding up
 * pushes the area past `maxArea`, both sides round down instead.
 */
function gridFit(tokenSide: number, maxArea: number, width: number, height: number): [number, number] {
  const side = Math.max(1, tokenSide);
  const area = width * height;
  const scale = area > maxArea ? Math.sqrt(maxArea / area) : 1;
  const nearest = (source: number) => {
    let squares = Math.round((source * scale) / side);
    if (squares * side > source) squares = Math.floor(source / side);
    return Math.max(1, squares) * side;
  };
  const [w, h] = [nearest(width), nearest(height)];
  if (w * h <= maxArea) return [w, h];
  const floor = (source: number) => Math.max(1, Math.floor((source * scale) / side)) * side;
  return [floor(width), floor(height)];
}

/** Scaled down so the long side is at most `limit`, aspect kept, never enlarged. */
function longSideWithin(limit: number, width: number, height: number): [number, number] {
  const long = Math.max(width, height);
  if (long <= limit || limit === 0) return [width, height];
  const scaled = (side: number) => Math.max(1, Math.floor((side * limit) / long));
  return [scaled(width), scaled(height)];
}

/** The size to send for a `width`×`height` source: downscaled to the geometry, never upscaled. */
export function fitPicture(geometry: VisionGeometry, width: number, height: number): [number, number] {
  const [w, h] = [Math.max(1, width), Math.max(1, height)];
  switch (geometry.kind) {
    case 'grid': return gridFit(geometry.token_side, geometry.max_pixels, w, h);
    case 'fixed': return longSideWithin(geometry.side, w, h);
    case 'tiles': return longSideWithin(geometry.max_side, w, h);
  }
}

/** Prompt tokens for a picture already at `fitPicture` size. */
export function pictureTokens(geometry: VisionGeometry, width: number, height: number): number {
  const [w, h] = [Math.max(1, width), Math.max(1, height)];
  switch (geometry.kind) {
    case 'grid': {
      const side = Math.max(1, geometry.token_side);
      const squares = sat(Math.max(1, Math.floor(w / side)) * Math.max(1, Math.floor(h / side)));
      return sat(Math.max(squares, minSquares(geometry)) + geometry.overhead_tokens);
    }
    case 'fixed':
      return geometry.tokens;
    case 'tiles': {
      const side = Math.max(1, geometry.tile_side);
      const tiles = Math.min(sat(Math.ceil(w / side) * Math.ceil(h / side)), Math.max(1, geometry.max_tiles));
      return sat(geometry.base_tokens + sat(geometry.tokens_per_tile * tiles));
    }
  }
}

export function planPicture(geometry: VisionGeometry, width: number, height: number): FittedPicture {
  const [w, h] = fitPicture(geometry, width, height);
  return { width: w, height: h, tokens: pictureTokens(geometry, w, h) };
}

/** The most a single picture can cost on this geometry. */
export function maxPictureTokens(geometry: VisionGeometry): number {
  switch (geometry.kind) {
    case 'grid': {
      const side = Math.max(1, geometry.token_side);
      const squares = Math.floor(geometry.max_pixels / (side * side));
      return sat(Math.max(squares, minSquares(geometry)) + geometry.overhead_tokens);
    }
    case 'fixed':
      return geometry.tokens;
    case 'tiles':
      return sat(geometry.base_tokens + sat(geometry.tokens_per_tile * Math.max(1, geometry.max_tiles)));
  }
}
