/**
 * The Settings preview and the backend must price a picture alike. Both
 * suites run every row of the backend's `vision/plan_cases.json`, whose
 * expected values are the Rust results; the hand-written tests below explain
 * the rules one at a time.
 */

import { describe, expect, it } from 'vitest';

import planCases from '../../../src-tauri/crates/lattice-core/src/domain/models/vision/plan_cases.json';
import {
  CONSERVATIVE_DEFAULT,
  VISION_PRESETS,
  cloudVisionDefault,
  geometryProblem,
  maxPictureTokens,
  planPicture,
  presetGeometry,
  presetOf,
  type VisionGeometry,
} from '../visionGeometry';

const fitted = (width: number, height: number, tokens: number) => ({ width, height, tokens });
const grid = (token_side: number, min_pixels: number, max_pixels: number): VisionGeometry =>
  ({ kind: 'grid', token_side, min_pixels, max_pixels, overhead_tokens: 2 });

describe('visionGeometry against the shared plan cases', () => {
  const geometries = planCases.geometries as unknown as Record<string, VisionGeometry>;

  it('names the same geometries as the code', () => {
    for (const preset of VISION_PRESETS) expect(geometries[preset.id]).toEqual(preset.geometry);
    expect(geometries.conservative_default).toEqual(CONSERVATIVE_DEFAULT);
  });

  it.each(planCases.cases)('$geometry $width×$height: $covers', ({ geometry, width, height, fitted }) => {
    expect(planPicture(geometries[geometry], width, height)).toEqual(fitted);
  });
});

describe('visionGeometry (rules from vision/tests.rs)', () => {
  const qwen25 = presetGeometry('qwen25_vl');
  const qwen3 = presetGeometry('qwen3_vl');
  const gemma = presetGeometry('gemma_siglip');
  const openai = presetGeometry('open_ai');
  const gemini = presetGeometry('gemini');

  it('a 28 px grid scales the area under its cap and rounds to the grid', () => {
    expect(planPicture(qwen25, 1280, 1600)).toEqual(fitted(896, 1120, 1282));
    expect(maxPictureTokens(qwen25)).toBe(1282);
  });

  it('a 32 px grid keeps a picture already under its cap', () => {
    expect(planPicture(qwen3, 1280, 1600)).toEqual(fitted(1280, 1600, 2002));
  });

  it('a grid never upscales and rounds down rather than past the source', () => {
    expect(planPicture(qwen25, 400, 300)).toEqual(fitted(392, 280, 142));
    expect(planPicture(qwen3, 400, 300)).toEqual(fitted(384, 288, 110));
  });

  it('a grid picture under the minimum area is charged as the minimum', () => {
    expect(planPicture(qwen3, 100, 100)).toEqual(fitted(96, 96, 66));
    expect(planPicture(qwen3, 10, 10)).toEqual(fitted(32, 32, 66));
  });

  it('a fixed encoder always costs the same', () => {
    expect(planPicture(gemma, 1280, 1600)).toEqual(fitted(716, 896, 256));
    expect(planPicture(gemma, 400, 300)).toEqual(fitted(400, 300, 256));
    expect(maxPictureTokens(gemma)).toBe(256);
  });

  it('tiles charge the overview plus each tile up to the tile limit', () => {
    expect(planPicture(openai, 1024, 1024)).toEqual(fitted(1024, 1024, 765));
    expect(planPicture(openai, 4096, 4096)).toEqual(fitted(2048, 2048, 2805));
    expect(maxPictureTokens(openai)).toBe(2805);
    expect(planPicture(gemini, 3072, 3072)).toEqual(fitted(3072, 3072, 1548));
    expect(planPicture(gemini, 300, 200)).toEqual(fitted(300, 200, 258));
  });

  it('the conservative default charges at least the local families and OpenAI for its square', () => {
    expect(maxPictureTokens(CONSERVATIVE_DEFAULT)).toBe(1026);
    for (const side of [896, 1792, 4000]) {
      const sent = planPicture(CONSERVATIVE_DEFAULT, side, side);
      expect(sent).toEqual(fitted(896, 896, 1026));
      for (const preset of ['qwen3_vl', 'qwen25_vl', 'gemma_siglip', 'open_ai'] as const) {
        expect(planPicture(presetGeometry(preset), sent.width, sent.height).tokens).toBeLessThanOrEqual(sent.tokens);
      }
    }
  });

  it('cloud models see by their providers documented families', () => {
    expect(cloudVisionDefault('openai', 'gpt-4o-mini')).toEqual(openai);
    expect(cloudVisionDefault('openai', 'gpt-5')).toEqual(openai);
    expect(cloudVisionDefault('openai', 'o3-mini')).toEqual(openai);
    expect(cloudVisionDefault('openai', 'gpt-3.5-turbo')).toBeNull();
    expect(cloudVisionDefault('anthropic', 'claude-sonnet-4-5')).toEqual(presetGeometry('anthropic'));
    expect(cloudVisionDefault('google', 'gemini-2.5-pro')).toEqual(gemini);
    expect(cloudVisionDefault('google', 'gemma-3-27b')).toBeNull();
  });

  it('presets are valid and degenerate declarations are named', () => {
    for (const preset of VISION_PRESETS) expect(geometryProblem(preset.geometry)).toBeNull();
    expect(geometryProblem(grid(0, 0, 100))).not.toBeNull();
    expect(geometryProblem(grid(28, 200, 100))).not.toBeNull();
    expect(geometryProblem({ kind: 'fixed', side: 0, tokens: 1 })).not.toBeNull();
    expect(geometryProblem({
      kind: 'tiles', tile_side: 0, tokens_per_tile: 1, base_tokens: 0, max_tiles: 1, max_side: 1,
    })).not.toBeNull();
  });
});

describe('presetOf', () => {
  it('names a preset geometry and leaves a custom one unnamed', () => {
    expect(presetOf(presetGeometry('gemini'))).toBe('gemini');
    expect(presetOf(grid(28, 0, 1000))).toBeNull();
  });
});
