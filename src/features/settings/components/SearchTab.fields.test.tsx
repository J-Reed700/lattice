import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchTab } from './SearchTab';

type Callbacks = { onError?: (error: Error) => void; onSettled?: () => void };

const h = vi.hoisted(() => ({
  query: {} as Record<string, any>,
  update: vi.fn(),
  reranker: undefined as unknown,
  download: { mutate: vi.fn(), isPending: false, error: null },
  toast: { error: vi.fn() },
  tuningProps: null as null | Record<string, any>,
}));
vi.mock('../hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => h.query,
  useUpdateSettingsMutation: () => ({ mutate: h.update }),
}));
vi.mock('../hooks/useRerankerQuery', () => ({
  useRerankerStatusQuery: () => ({ data: h.reranker }),
  useDownloadRerankerMutation: () => h.download,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('./RetrievalTuningSection', () => ({
  RetrievalTuningSection: (props: Record<string, any>) => { h.tuningProps = props; return null; },
}));

const search = (overrides: Record<string, unknown> = {}) => ({
  enableReranking: true,
  maxResults: 10,
  similarityThreshold: 0.3,
  hybridSearchAlpha: 0.5,
  summaryIndexEnabled: false,
  vectorIndexCompression: { mode: 'none', dims: 256, quantization: 'f32' },
  retrievalTuning: { readWebPages: true, sufficiencyRetryEnabled: false },
  ...overrides,
});

const last = (): [Record<string, unknown>, Callbacks] => h.update.mock.calls.at(-1) as never;
const edit = (label: string, value: string) => {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.blur(screen.getByLabelText(label));
};

describe('SearchTab fields', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.query = { data: { search: search() }, isPending: false, refetch: vi.fn() };
    h.reranker = { installed: true, downloadBytes: 0 };
    h.download = { mutate: vi.fn(), isPending: false, error: null };
  });

  it('shows loading, then a retry when settings cannot be read', () => {
    h.query = { data: undefined, isPending: true, refetch: vi.fn() };
    const loading = render(<SearchTab />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    loading.unmount();
    h.query = { data: { search: null }, isPending: false, refetch: vi.fn() };
    render(<SearchTab />);
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(h.query.refetch).toHaveBeenCalled();
  });

  it('offers the reranker download, and shows it in progress', () => {
    h.reranker = { installed: false, downloadBytes: 120 * 1024 * 1024 };
    const first = render(<SearchTab />);
    fireEvent.click(screen.getByRole('button', { name: /^Download \(/ }));
    expect(h.download.mutate).toHaveBeenCalled();
    first.unmount();
    h.download = { mutate: vi.fn(), isPending: true, error: null };
    render(<SearchTab />);
    expect(screen.getByRole('button', { name: 'Downloading…' })).toBeDisabled();
  });

  it('saves switches straight away', () => {
    render(<SearchTab />);
    fireEvent.click(screen.getByLabelText('Read web pages'));
    expect(last()[0]).toEqual({ category: 'search', updates: { retrievalTuning: { readWebPages: false, sufficiencyRetryEnabled: false } } });
    fireEvent.click(screen.getByLabelText('Corrective retry'));
    expect(last()[0]).toEqual({ category: 'search', updates: { retrievalTuning: { readWebPages: true, sufficiencyRetryEnabled: true } } });
    fireEvent.click(screen.getByLabelText('Document summaries'));
    expect(last()[0]).toEqual({ category: 'search', updates: { summaryIndexEnabled: true } });
  });

  it('saves numbers on blur, clamped, and drops the draft once settled', () => {
    render(<SearchTab />);
    edit('Maximum results', '500');
    expect(last()[0]).toEqual({ category: 'search', updates: { maxResults: 128 } });
    expect(screen.getByLabelText('Maximum results')).toHaveValue(500);
    act(() => last()[1].onSettled?.());
    expect(screen.getByLabelText('Maximum results')).toHaveValue(10);
    edit('Similarity threshold', '1.5');
    expect(last()[0]).toEqual({ category: 'search', updates: { similarityThreshold: 1 } });
    edit('Hybrid alpha', '0.75');
    expect(last()[0]).toEqual({ category: 'search', updates: { hybridSearchAlpha: 0.75 } });
    last()[1].onError?.(new Error('settings locked'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't save search settings", { message: 'settings locked' });
  });

  it('leaves drafts alone when none were held', () => {
    render(<SearchTab />);
    fireEvent.click(screen.getByLabelText('Document summaries'));
    act(() => last()[1].onSettled?.());
    expect(screen.getByLabelText('Maximum results')).toHaveValue(10);
  });

  it('configures truncated vector storage', () => {
    h.query = { data: { search: search({ vectorIndexCompression: { mode: 'truncated', dims: 256, quantization: 'f32' } }) }, isPending: false, refetch: vi.fn() };
    render(<SearchTab />);
    edit('Stored dimensions', '10');
    expect(last()[0]).toEqual({ category: 'search', updates: { vectorIndexCompression: { mode: 'truncated', dims: 32, quantization: 'f32' } } });
    fireEvent.change(screen.getByLabelText('Stored precision'), { target: { value: 'i8' } });
    expect(last()[0]).toEqual({ category: 'search', updates: { vectorIndexCompression: { mode: 'truncated', dims: 256, quantization: 'i8' } } });
    fireEvent.change(screen.getByLabelText('Vector storage'), { target: { value: 'none' } });
    expect(last()[0]).toEqual({ category: 'search', updates: { vectorIndexCompression: { mode: 'none', dims: 256, quantization: 'f32' } } });
  });

  it('hands the advanced tuning its drafts and save', () => {
    render(<SearchTab />);
    expect(h.tuningProps?.showAdvancedTuning).toBe(false);
    act(() => h.tuningProps?.setShowAdvancedTuning(true));
    expect(h.tuningProps?.showAdvancedTuning).toBe(true);
    act(() => h.tuningProps?.setDraft('docShortlistDocMin', 4));
    expect(h.tuningProps?.drafts).toEqual({ docShortlistDocMin: 4 });
    act(() => h.tuningProps?.clearDrafts(['docShortlistDocMin']));
    expect(h.tuningProps?.drafts).toEqual({});
  });
});
