import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ApiClient } from '@/lib/api';

import { readSourceBytes } from '../readSourceFile';

const mocks = vi.hoisted(() => ({
  assetUrl: vi.fn((path: string) => `asset://localhost/${encodeURIComponent(path)}`),
  readFileBytes: vi.fn(),
}));

vi.mock('@/utils/assetUrl', () => ({
  assetUrl: mocks.assetUrl,
}));

vi.mock('@/lib/api', () => {
  const api = { readFileBytes: mocks.readFileBytes };
  return { default: api, VaultAPI: api };
});

function setTauriRuntime(enabled: boolean): void {
  const tauriWindow = window as Window & { __TAURI_INTERNALS__?: unknown };
  if (enabled) {
    tauriWindow.__TAURI_INTERNALS__ = {};
  } else {
    delete tauriWindow.__TAURI_INTERNALS__;
  }
}

describe('readSourceBytes', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    vi.clearAllMocks();
    setTauriRuntime(false);
    mocks.readFileBytes.mockResolvedValue({ ok: true, data: new Uint8Array([9, 8]) });
  });

  afterEach(() => {
    setTauriRuntime(false);
    globalThis.fetch = originalFetch;
  });

  it('streams binary bytes directly via assetUrl when running in Tauri with default VaultAPI', async () => {
    setTauriRuntime(true);
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      arrayBuffer: async () => bytes.buffer,
    });
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const result = await readSourceBytes('/vault/paper.pdf');

    expect(mocks.assetUrl).toHaveBeenCalledWith('/vault/paper.pdf');
    expect(fetchMock).toHaveBeenCalledWith('asset://localhost/%2Fvault%2Fpaper.pdf');
    expect(mocks.readFileBytes).not.toHaveBeenCalled();
    expect(result).toEqual({ ok: true, data: bytes });
  });

  it('falls back to IPC readFileBytes when assetUrl fetch fails with non-ok status', async () => {
    setTauriRuntime(true);
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 403 });
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const result = await readSourceBytes('/vault/paper.pdf');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.readFileBytes).toHaveBeenCalledWith('/vault/paper.pdf');
    expect(result).toEqual({ ok: true, data: new Uint8Array([9, 8]) });
  });

  it('falls back to IPC readFileBytes when assetUrl fetch rejects', async () => {
    setTauriRuntime(true);
    const fetchMock = vi.fn().mockRejectedValue(new TypeError('network error'));
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const result = await readSourceBytes('/vault/paper.pdf');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.readFileBytes).toHaveBeenCalledWith('/vault/paper.pdf');
    expect(result).toEqual({ ok: true, data: new Uint8Array([9, 8]) });
  });

  it('uses api.readFileBytes directly outside Tauri or with a custom injected ApiClient', async () => {
    const fetchMock = vi.fn();
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await readSourceBytes('/vault/paper.pdf');
    expect(fetchMock).not.toHaveBeenCalled();
    expect(mocks.readFileBytes).toHaveBeenCalledWith('/vault/paper.pdf');

    setTauriRuntime(true);
    const customApi = {
      readFileBytes: vi.fn().mockResolvedValue({ ok: true, data: new Uint8Array([7]) }),
    } as unknown as ApiClient;

    const customResult = await readSourceBytes('/vault/custom.docx', customApi);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(customApi.readFileBytes).toHaveBeenCalledWith('/vault/custom.docx');
    expect(customResult).toEqual({ ok: true, data: new Uint8Array([7]) });
  });
});
