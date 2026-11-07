import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { router } from '@/routes';

import { ModelSetupModal } from './ModelSetupModal';

const mocks = vi.hoisted(() => ({
  api: { checkFirstRunStatus: vi.fn(), recommendChatModels: vi.fn(), downloadDefaultEmbeddingModel: vi.fn(), downloadModel: vi.fn() },
}));
vi.mock('@/routes', () => ({ router: { navigate: vi.fn() } }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => ({ systemCapabilities: null, compatibleModels: [] }) }));
vi.mock('@/lib/api', () => ({ default: mocks.api, VaultAPI: mocks.api }));

const GB = 1_073_741_824;
const RTX = { name: 'NVIDIA GeForce RTX 4080', kind: 'dedicated', totalBytes: 16 * GB };

const status = () => JSON.stringify({
  needs_setup: true,
  embedding_model: { model_id: 'bge-small', display_name: 'BGE Small', estimated_size_bytes: 134_217_728 },
  chat_model: { model_id: 'qwen-9b', display_name: 'Qwen 3.5 9B', estimated_size_bytes: 5.7 * GB },
  total_estimated_size_bytes: 5.9 * GB,
});

const recommendation = (overrides: Record<string, unknown> = {}) => ({
  modelId: 'qwen-9b',
  name: 'Qwen 3.5 9B',
  description: 'Strong all-round chat model.',
  downloadBytes: 5.7 * GB,
  verdict: { kind: 'fits', needsBytes: 11.2 * GB, budgetBytes: 15 * GB, device: RTX },
  labels: ['best'],
  evidence: { kind: 'measured', suites: ['course eval'] },
  speed: 'balanced',
  ...overrides,
});

const recommendations = () => [
  recommendation(),
  recommendation({
    modelId: 'gemma-4b',
    name: 'Gemma 4 4B',
    description: 'Small and quick.',
    downloadBytes: 2.9 * GB,
    verdict: { kind: 'fits', needsBytes: 5.3 * GB, budgetBytes: 15 * GB, device: RTX },
    labels: ['fastest', 'smallest_download'],
    evidence: { kind: 'untested' },
    speed: 'fast',
  }),
  recommendation({
    modelId: 'herb-12b',
    name: 'Herb 12B',
    description: 'Larger and slower.',
    downloadBytes: 7.4 * GB,
    verdict: { kind: 'fits', needsBytes: 13.6 * GB, budgetBytes: 15 * GB, device: RTX },
    labels: [],
    evidence: { kind: 'untested' },
    speed: 'thorough',
  }),
];

function renderModal() {
  const props = { onOpenChange: vi.fn(), onComplete: vi.fn(), onDismiss: vi.fn() };
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  render(<ModelSetupModal open {...props} />, { wrapper });
  return props;
}

const cards = async () => within(await screen.findByRole('radiogroup', { name: 'Recommended for this computer' })).getAllByRole('radio');
const install = () => screen.findByRole('button', { name: /^Install/ });

beforeEach(() => {
  mocks.api.checkFirstRunStatus.mockReset().mockResolvedValue({ ok: true, data: status() });
  mocks.api.recommendChatModels.mockReset().mockResolvedValue({ ok: true, data: recommendations() });
  mocks.api.downloadDefaultEmbeddingModel.mockReset().mockResolvedValue({ ok: true, data: 'embed-job' });
  mocks.api.downloadModel.mockReset().mockResolvedValue({ ok: true, data: { id: 'chat-job' } });
  vi.mocked(router.navigate).mockReset();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe('ModelSetupModal recommended list', () => {
  it('lists one card per recommendation in the backend order, the first selected', async () => {
    renderModal();
    const radios = await cards();
    expect(radios).toHaveLength(3);
    expect(radios[0]).toHaveAccessibleName(/Qwen 3\.5 9B/);
    expect(radios[1]).toHaveAccessibleName(/Gemma 4 4B/);
    expect(radios[2]).toHaveAccessibleName(/Herb 12B/);
    expect(radios[0]).toBeChecked();
    expect(radios[1]).not.toBeChecked();
    expect(radios[2]).not.toBeChecked();
  });

  it('shows each card’s download size and what it uses of which device', async () => {
    renderModal();
    const radios = await cards();
    expect(radios[0]).toHaveAccessibleName(/5\.7 GB/);
    expect(radios[0]).toHaveAccessibleName(/Uses 11\.2 of 16 GB on NVIDIA GeForce RTX 4080/);
    expect(radios[1]).toHaveAccessibleName(/2\.9 GB/);
    expect(radios[1]).toHaveAccessibleName(/Uses 5\.3 of 16 GB on NVIDIA GeForce RTX 4080/);
    expect(radios[2]).toHaveAccessibleName(/Uses 13\.6 of 16 GB on NVIDIA GeForce RTX 4080/);
  });

  it('shows the labels the backend gave each card, and Tested by Lattice only when measured', async () => {
    renderModal();
    const radios = await cards();
    expect(radios[0]).toHaveAccessibleName(/Best for this computer/);
    expect(radios[0]).toHaveAccessibleName(/Tested by Lattice/);
    expect(radios[0]).not.toHaveAccessibleName(/Fastest/);
    expect(radios[0]).not.toHaveAccessibleName(/Smallest download/);

    expect(radios[1]).toHaveAccessibleName(/Fastest/);
    expect(radios[1]).toHaveAccessibleName(/Smallest download/);
    expect(radios[1]).not.toHaveAccessibleName(/Best for this computer/);
    expect(radios[1]).not.toHaveAccessibleName(/Tested by Lattice/);

    expect(radios[2]).not.toHaveAccessibleName(/Best for this computer|Fastest|Smallest download|Tested by Lattice/);
  });

  it('says each card’s speed in words, not the backend’s code', async () => {
    renderModal();
    const radios = await cards();
    expect(radios[0]).toHaveAccessibleName(/Medium/);
    expect(radios[1]).toHaveAccessibleName(/Fast/);
    expect(radios[2]).toHaveAccessibleName(/Slower/);
    for (const radio of radios) expect(radio).not.toHaveAccessibleName(/balanced|thorough|Speed: fast/);
  });

  it('says a card runs on the processor, slower, when there is no usable graphics card', async () => {
    const cpu = { name: 'AMD Ryzen 7 5800X', kind: 'processor', totalBytes: 32 * GB };
    mocks.api.recommendChatModels.mockResolvedValue({
      ok: true,
      data: recommendations().map((entry) => ({ ...entry, verdict: { ...entry.verdict, device: cpu } })),
    });
    renderModal();
    const radios = await cards();
    for (const radio of radios) expect(radio).toHaveAccessibleName(/Runs on the processor: slower/);
  });

  it('does not call a card slow when it runs on a graphics card', async () => {
    renderModal();
    for (const radio of await cards()) expect(radio).not.toHaveAccessibleName(/Runs on the processor/);
  });

  it('installs the first card’s model and the embedding model by default', async () => {
    renderModal();
    await cards();
    fireEvent.click(await install());
    await waitFor(() => expect(mocks.api.downloadDefaultEmbeddingModel).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(mocks.api.downloadModel).toHaveBeenCalledWith('qwen-9b'));
    expect(mocks.api.downloadModel).toHaveBeenCalledTimes(1);
  });

  it('installs the chosen card’s model, and the embedding model, not the first card’s', async () => {
    const props = renderModal();
    const radios = await cards();
    fireEvent.click(radios[1]);
    expect((await cards())[1]).toBeChecked();
    expect((await cards())[0]).not.toBeChecked();
    fireEvent.click(await install());
    await waitFor(() => expect(mocks.api.downloadModel).toHaveBeenCalledWith('gemma-4b'));
    await waitFor(() => expect(mocks.api.downloadDefaultEmbeddingModel).toHaveBeenCalledTimes(1));
    expect(mocks.api.downloadModel).toHaveBeenCalledTimes(1);
    expect(mocks.api.downloadModel).not.toHaveBeenCalledWith('qwen-9b');
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onComplete).toHaveBeenCalled();
  });

  it('still skips for now, remembering the offer', async () => {
    const props = renderModal();
    await cards();
    fireEvent.click(screen.getByRole('button', { name: 'Not now' }));
    expect(props.onDismiss).toHaveBeenCalled();
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onComplete).toHaveBeenCalled();
    expect(mocks.api.downloadModel).not.toHaveBeenCalled();
  });

  it('still opens the full catalog from See all models', async () => {
    const props = renderModal();
    await cards();
    fireEvent.click(screen.getByRole('button', { name: 'See all models' }));
    expect(props.onDismiss).toHaveBeenCalled();
    expect(router.navigate).toHaveBeenCalledWith('/settings?tab=models');
    expect(props.onComplete).toHaveBeenCalled();
  });
});
