import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { modelApi } from '@/features/model/api/client';

import { ModelVariantPicker } from './ModelVariantPicker';
import { modelQuantization } from './quantization';

import type { ModelMetadata } from '../../../types/modelCatalog';

vi.mock('@/features/model/api/client', () => ({ modelApi: { getModelVariants: vi.fn() } }));
const base: ModelMetadata = {
  id: 'q4', name: 'Example 8B', model_id: 'publisher/example', category: 'LLM', description: '',
  default_filename: 'model-Q4_K_M.gguf', supported_quantizations: ['Q4_K_M', 'Q8_0'],
  size_gb: 4, total_size_bytes: 4_000_000_000, minimum_ram_gb: 6, recommended_ram_gb: 8,
  context_length: 8192, performance_tier: 'Balanced', capabilities: ['chat'],
  license: 'MIT', requires_auth: false, download_url: null, files: [],
  embedding_dimensions: null, embedding_compatibility: null,
};
const q8 = { ...base, id: 'q8', default_filename: 'model-Q8_0.gguf', supported_quantizations: ['Q8_0'], size_gb: 8, minimum_ram_gb: 12 };
function setup() {
  const onSelect = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><ModelVariantPicker model={base} selected={base} capabilities={null} onSelect={onSelect} /></QueryClientProvider>);
  return { user: userEvent.setup(), onSelect };
}

describe('published model versions', () => {
  beforeEach(() => vi.resetAllMocks());

  it('shows an explicit loading state and selects the exact published artifact', async () => {
    let finish!: (value: Awaited<ReturnType<typeof modelApi.getModelVariants>>) => void;
    vi.mocked(modelApi.getModelVariants).mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const { user, onSelect } = setup();
    expect(screen.getByRole('status')).toHaveTextContent('Loading published files');
    expect(modelApi.getModelVariants).toHaveBeenCalledWith('publisher/example');
    await act(async () => finish({ ok: true, data: [base, q8] }));
    await user.type(await screen.findByLabelText('Filter versions'), 'Q8');
    expect(screen.getByRole('status')).toHaveTextContent('1 of 2 standalone versions');
    const choice = screen.getByRole('button', { name: /Q8_0/ });
    expect(choice).toHaveTextContent('8.0 GB');
    expect(choice).toHaveTextContent('Est. memory 12.0 GB');
    await user.click(choice);
    expect(onSelect).toHaveBeenCalledWith(q8);
  });

  it('distinguishes a failed lookup from an empty repository and allows retry', async () => {
    vi.mocked(modelApi.getModelVariants).mockResolvedValueOnce({ ok: false, error: 'Offline' }).mockResolvedValueOnce({ ok: true, data: [] });
    const { user } = setup();
    expect(await screen.findByRole('alert')).toHaveTextContent('Offline');
    expect(screen.queryByText(/No standalone GGUF/)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry versions' }));
    expect(await screen.findByText('No standalone GGUF downloads are available in this repository.')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('keeps unreported sizes unknown and never fabricates a fit', async () => {
    vi.mocked(modelApi.getModelVariants).mockResolvedValue({ ok: true, data: [{ ...base, size_gb: 0, minimum_ram_gb: 0 }] });
    setup();
    expect(await screen.findByText('Size unavailable')).toBeInTheDocument();
    expect(screen.getByText('Memory estimate unavailable')).toBeInTheDocument();
    expect(screen.queryByText(/Fits on this computer/)).not.toBeInTheDocument();
  });

  it('keeps an installed default’s catalog identity when refreshing its file metadata', async () => {
    vi.mocked(modelApi.getModelVariants).mockResolvedValue({ ok: true, data: [{ ...base, id: 'hf-default', size_gb: 4.5 }] });
    const { onSelect } = setup();
    await screen.findByText('4.5 GB');
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: base.id, size_gb: 4.5, default_filename: base.default_filename }));
  });

  it.each(['Q4_0', 'IQ4_XS', 'Q6_K', 'UD-Q5_K_XL', 'BF16'])('reads %s from the selected filename instead of repository tags', (precision) => {
    expect(modelQuantization({ ...base, default_filename: `model-${precision}.gguf` })).toBe(precision);
  });
  it('does not infer precision from an unlabelled GGUF', () => {
    expect(modelQuantization({ ...base, default_filename: 'model.gguf', supported_quantizations: [] })).toBeNull();
  });
});
