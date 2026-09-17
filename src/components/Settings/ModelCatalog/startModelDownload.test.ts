import { QueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';

import { startModelDownload } from './startModelDownload';

import type { ModelMetadata } from '../../../types/modelCatalog';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('startModelDownload', () => {
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
    expect(invoke).not.toHaveBeenCalled();
    expect(addToast).toHaveBeenCalledWith({
      type: 'warning',
      title: 'Model not supported',
      message: 'The local runtime cannot load sharded weights yet.',
      duration: 8000,
    });
  });
});
