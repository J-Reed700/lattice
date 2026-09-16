import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { modelMetaLine } from './catalogUtils';
import { ModelFilterPanel } from './ModelFilterPanel';
import { ModelRow } from './ModelRow';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';

import type { ModelMetadata, ModelRecommendation } from '../../../types/modelCatalog';

const metadata = (overrides: Partial<ModelMetadata> = {}): ModelMetadata => ({
  id: 'qwen2.5-7b-instruct-q4_k_m',
  name: 'Qwen 2.5 7B Instruct',
  category: 'LLM',
  description: 'A general-purpose instruction model.',
  size_gb: 4.4,
  minimum_ram_gb: 8,
  recommended_ram_gb: 16,
  context_length: 32768,
  performance_tier: 'Balanced',
  supported_quantizations: ['Q4_K_M'],
  capabilities: ['chat'],
  download_url: null,
  license: 'Apache-2.0',
  requires_auth: false,
  model_id: 'Qwen/Qwen2.5-7B-Instruct-GGUF',
  default_filename: 'qwen2.5-7b-instruct-q4_k_m.gguf',
  files: [],
  total_size_bytes: 4_724_464_025,
  embedding_dimensions: null,
  embedding_compatibility: null,
  ...overrides,
});

const recommendation = (overrides: Partial<ModelMetadata> = {}): ModelRecommendation => ({
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
});

const resetStore = () =>
  useModelCatalogStore.setState({
    filters: {
      category: null,
      max_size_gb: null,
      min_downloads: null,
      required_capabilities: [],
      query_text: null,
      embedding_dimensions: null,
    },
    searchQuery: '',
    sortBy: 'popularity',
    selectedModel: null,
  });

describe('modelMetaLine', () => {
  it('reads as one sentence-case line of the fields that exist', () => {
    expect(modelMetaLine(metadata(), 12_000)).toBe('4.4 GB · 12K downloads · Chat · Q4_K_M');
  });

  it('omits missing fields rather than printing "Unknown"', () => {
    const line = modelMetaLine(metadata({ size_gb: 0, supported_quantizations: [] }), null);
    expect(line).toBe('Chat');
  });

  it('turns the safetensors badge into a word in the meta line', () => {
    const line = modelMetaLine(metadata({ format: 'safetensors' }), 12_000);
    expect(line).toContain('Safetensors');
  });
});

describe('ModelFilterPanel', () => {
  beforeEach(resetStore);

  it('calls the LLM category "Chat" and offers an All tab', () => {
    render(<ModelFilterPanel />);
    expect(screen.getByRole('tab', { name: 'All' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Chat' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'LLM' })).not.toBeInTheDocument();
  });

  it('has no filters heading and no pills', () => {
    render(<ModelFilterPanel />);
    expect(screen.queryByText('Filters')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Maximum size')).toHaveValue('');
    expect(screen.getByLabelText('Minimum downloads')).toBeInTheDocument();
    expect(screen.getByLabelText('Speed')).toBeInTheDocument();
  });

  it('shows the dimensions select only for embedding models', async () => {
    const user = userEvent.setup();
    render(<ModelFilterPanel />);
    expect(screen.queryByLabelText('Embedding dimensions')).not.toBeInTheDocument();

    await user.click(screen.getByRole('tab', { name: 'Embedding' }));
    expect(screen.getByLabelText('Embedding dimensions')).toBeInTheDocument();
  });

  it('offers Reset filters only once a filter is set, and clears them', async () => {
    const user = userEvent.setup();
    render(<ModelFilterPanel />);
    expect(screen.queryByRole('button', { name: 'Reset filters' })).not.toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText('Maximum size'), '4');
    expect(useModelCatalogStore.getState().filters.max_size_gb).toBe(4);

    await user.click(screen.getByRole('button', { name: 'Reset filters' }));
    expect(useModelCatalogStore.getState().filters.max_size_gb).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reset filters' })).not.toBeInTheDocument();
  });
});

describe('ModelRow', () => {
  const rowProps = {
    isDownloaded: false,
    activeDownload: null,
    isStarting: false,
    onDownload: vi.fn(),
    onSelect: vi.fn(),
  };

  it('shows the name, the repo id, and one meta line', () => {
    render(<ModelRow model={recommendation()} {...rowProps} />);
    expect(screen.getByText('Qwen 2.5 7B Instruct')).toBeInTheDocument();
    expect(screen.getByText('Qwen/Qwen2.5-7B-Instruct-GGUF')).toBeInTheDocument();
    expect(screen.getByText('4.4 GB · 12K downloads · Chat · Q4_K_M')).toBeInTheDocument();
  });

  it('opens the detail view when the row text is clicked', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<ModelRow model={recommendation()} {...rowProps} onSelect={onSelect} />);

    await user.click(screen.getByText('Qwen 2.5 7B Instruct'));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it('starts a download from the row', async () => {
    const user = userEvent.setup();
    const onDownload = vi.fn();
    render(<ModelRow model={recommendation()} {...rowProps} onDownload={onDownload} />);

    await user.click(screen.getByRole('button', { name: 'Download' }));
    expect(onDownload).toHaveBeenCalledTimes(1);
  });

  it('replaces the button with muted text once downloaded', () => {
    render(<ModelRow model={recommendation()} {...rowProps} isDownloaded />);
    expect(screen.queryByRole('button', { name: 'Download' })).not.toBeInTheDocument();
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
  });

  it('shows progress while a download is in flight', () => {
    render(
      <ModelRow
        model={recommendation()}
        {...rowProps}
        activeDownload={
          {
            id: 'd-1',
            url: 'https://example.invalid/model.gguf',
            destination: '/tmp/model.gguf',
            state: 'Downloading',
            bytes_downloaded: 100,
            total_bytes: 200,
            bytes_per_second: 10,
            percentage: 42,
            eta_seconds: 10,
            error_message: null,
            retry_count: 0,
            created_at: '2026-09-05T00:00:00Z',
            started_at: '2026-09-05T00:00:00Z',
            completed_at: null,
            model_name: null,
            model_id: null,
          }
        }
      />,
    );
    expect(screen.getByText('Downloading 42%')).toBeInTheDocument();
  });

  it('offers the token on a gated row', async () => {
    const user = userEvent.setup();
    const onAddToken = vi.fn();
    render(
      <ModelRow
        model={recommendation({ requires_auth: true })}
        {...rowProps}
        hasHfToken={false}
        onAddToken={onAddToken}
      />,
    );

    await user.click(screen.getByRole('button', { name: 'Token required · Add token' }));
    expect(onAddToken).toHaveBeenCalledTimes(1);
  });

  it('still offers the token when the gated row has no file to fetch yet', () => {
    // No `model_id`/`default_filename`/`download_url`: not downloadable, but a
    // token is what unblocks it, so the row must not go silent.
    render(
      <ModelRow
        model={recommendation({
          requires_auth: true,
          model_id: null,
          default_filename: null,
          download_url: null,
        })}
        {...rowProps}
        hasHfToken={false}
        onAddToken={vi.fn()}
      />,
    );

    expect(
      screen.getByRole('button', { name: 'Token required · Add token' }),
    ).toBeInTheDocument();
  });

  it('goes back to Download once a token is stored', () => {
    render(
      <ModelRow
        model={recommendation({ requires_auth: true })}
        {...rowProps}
        hasHfToken
        onAddToken={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Download' })).toBeInTheDocument();
  });
});
