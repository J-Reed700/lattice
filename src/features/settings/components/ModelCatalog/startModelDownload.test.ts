import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VaultAPI } from '@/lib/api';
import type { ModelMetadata } from '@/types/modelCatalog';

import { startModelDownload } from './startModelDownload';

const chatModel: ModelMetadata = {
  id: 'qwen3-4b',
  name: 'Qwen3 4B',
  category: 'LLM',
  description: '',
  size_gb: 2.5,
  minimum_ram_gb: 4,
  recommended_ram_gb: 8,
  context_length: 32768,
  performance_tier: 'Balanced',
  supported_quantizations: ['Q4_K_M'],
  capabilities: ['chat'],
  download_url: 'https://huggingface.co/Qwen/Qwen3-4B-GGUF',
  license: 'Apache-2.0',
  requires_auth: false,
  model_id: 'Qwen/Qwen3-4B-GGUF',
  default_filename: null,
  files: [],
  total_size_bytes: 0,
  embedding_dimensions: null,
  embedding_compatibility: null,
  format: 'gguf',
};

describe('startModelDownload', () => {
  beforeEach(() => {
    vi.mocked(VaultAPI.downloadModel).mockClear();
  });

  it('asks the user to wait when the backend rate-limits downloads', async () => {
    vi.mocked(VaultAPI.downloadModel).mockResolvedValueOnce({
      ok: false,
      error: 'Rate limit reached for model downloads',
    });
    const addToast = vi.fn(() => 'toast-id');

    const result = await startModelDownload({
      metadata: chatModel,
      addToast,
      queryClient: new QueryClient(),
    });

    expect(result).toEqual({ alreadyDownloaded: false });
    expect(VaultAPI.downloadModel).toHaveBeenCalledWith('qwen3-4b');
    expect(addToast).toHaveBeenCalledWith({
      type: 'warning',
      title: 'Too many downloads at once',
      message: 'Wait a moment, then try again.',
    });
  });

  it('blocks unsupported embedding models and explains why', async () => {
    const addToast = vi.fn(() => 'toast-id');
    const metadata: ModelMetadata = {
      id: 'qwen-qwen3-embedding-4b',
      name: 'Qwen3 Embedding 4B',
      category: 'Embedding',
      description: '',
      size_gb: 0,
      minimum_ram_gb: 4,
      recommended_ram_gb: 8,
      context_length: 4096,
      performance_tier: 'Balanced',
      supported_quantizations: ['F16'],
      capabilities: ['embedding'],
      download_url: 'https://huggingface.co/Qwen/Qwen3-Embedding-4B',
      license: 'Apache-2.0',
      requires_auth: false,
      model_id: 'Qwen/Qwen3-Embedding-4B',
      default_filename: null,
      files: [],
      total_size_bytes: 0,
      embedding_dimensions: null,
      embedding_compatibility: {
        kind: 'incompatible',
        architecture: 'qwen3',
        reason: 'The local runtime cannot load sharded weights yet.',
      },
      format: 'safetensors',
    };

    const result = await startModelDownload({
      metadata,
      addToast,
      queryClient: new QueryClient(),
    });

    expect(result).toEqual({ alreadyDownloaded: false });
    expect(VaultAPI.downloadModel).not.toHaveBeenCalled();
    expect(addToast).toHaveBeenCalledWith({
      type: 'warning',
      title: 'Model not supported',
      message: 'The local runtime cannot load sharded weights yet.',
      duration: 8000,
    });
  });
});
