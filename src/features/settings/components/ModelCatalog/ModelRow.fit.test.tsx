import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ModelMetadata, ModelRecommendation } from '@/types/modelCatalog';

import { computeModelFit } from './catalogUtils';
import { ModelRow } from './ModelRow';

const GB = 1_073_741_824;
const RTX = { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated' };

const metadata = (overrides: Partial<ModelMetadata> = {}): ModelMetadata => ({
  id: 'qwen3.5-9b-q4_k_m',
  name: 'Qwen 3.5 9B',
  category: 'LLM',
  description: 'A general-purpose model.',
  size_gb: 5.7,
  minimum_ram_gb: 8,
  recommended_ram_gb: 16,
  context_length: 32768,
  performance_tier: 'Balanced',
  supported_quantizations: ['Q4_K_M'],
  capabilities: ['chat'],
  download_url: null,
  license: 'Apache-2.0',
  requires_auth: false,
  model_id: 'Qwen/Qwen3.5-9B-GGUF',
  default_filename: 'qwen3.5-9b-q4_k_m.gguf',
  files: [],
  total_size_bytes: 6_120_328_397,
  embedding_dimensions: null,
  embedding_compatibility: null,
  ...overrides,
});

const row = (fit: unknown, overrides: Partial<ModelMetadata> = {}) => ({
  model: metadata(overrides),
  compatibility: {
    compatibility_level: 'Excellent',
    overall_score: 94,
    ram_score: 98,
    gpu_score: 92,
    disk_score: 99,
    estimated_tokens_per_second: 34.5,
    estimated_loading_time_seconds: 4.2,
    recommendations: [],
    blockers: [],
  },
  ranking_score: 92,
  popularity_downloads: 12_000,
  popularity_likes: 310,
  fit,
}) as ModelRecommendation;

const verdict = (kind: 'fits' | 'tight' | 'too_large') => ({
  kind, needsBytes: 11.2 * GB, budgetBytes: 15 * GB, device: RTX,
});

const rowProps = { isDownloaded: false, activeDownload: null, isStarting: false, onDownload: vi.fn(), onSelect: vi.fn() };

// A machine on which the old frontend rule would say "Fits" for an 8 GB model.
const ROOMY = { total_ram_gb: 64, gpu_acceleration: 'None', gpu_type: 'None', vram_gb: null, available_disk_gb: 500 };
// ...and one on which it would say "Too large".
const CRAMPED = { total_ram_gb: 4, gpu_acceleration: 'None', gpu_type: 'None', vram_gb: null, available_disk_gb: 500 };
const withCapabilities = (capabilities: unknown) => ({ capabilities }) as object;

describe('ModelRow fit label', () => {
  it.each([
    ['fits', 'Fits'],
    ['tight', 'Fits with a shorter context'],
    ['too_large', 'Too large for this computer'],
  ] as const)('shows the backend verdict %s as "%s"', (kind, label) => {
    render(<ModelRow model={row(verdict(kind))} {...rowProps} />);
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it('says Checking size while the backend has not read the size', () => {
    render(<ModelRow model={row(null)} {...rowProps} />);
    expect(screen.getByText('Checking size')).toBeInTheDocument();
    expect(screen.queryByText('Fits')).not.toBeInTheDocument();
  });

  it('names the device the verdict was judged against in its tooltip', () => {
    render(<ModelRow model={row(verdict('fits'))} {...rowProps} />);
    expect(screen.getByText('Fits').closest('[title]')?.getAttribute('title')).toContain('NVIDIA GeForce RTX 4080');
  });

  it('shows the backend’s "Too large" where the old frontend rule says "Fits"', () => {
    expect(computeModelFit(metadata(), ROOMY as never)?.verdict).toBe('fits');
    render(<ModelRow model={row(verdict('too_large'))} {...rowProps} {...withCapabilities(ROOMY)} />);
    expect(screen.getByText('Too large for this computer')).toBeInTheDocument();
    expect(screen.queryByText('Fits')).not.toBeInTheDocument();
  });

  it('shows the backend’s "Fits" where the old frontend rule says "Too large"', () => {
    expect(computeModelFit(metadata(), CRAMPED as never)?.verdict).toBe('too-large');
    render(<ModelRow model={row(verdict('fits'))} {...rowProps} {...withCapabilities(CRAMPED)} />);
    expect(screen.getByText('Fits')).toBeInTheDocument();
    expect(screen.queryByText('Too large for this computer')).not.toBeInTheDocument();
    expect(screen.queryByText('Too large')).not.toBeInTheDocument();
  });

  it('says Checking size, not a computed verdict, when the backend sent none', () => {
    render(<ModelRow model={row(null)} {...rowProps} {...withCapabilities(ROOMY)} />);
    expect(screen.getByText('Checking size')).toBeInTheDocument();
    expect(screen.queryByText('Fits')).not.toBeInTheDocument();
  });
});
