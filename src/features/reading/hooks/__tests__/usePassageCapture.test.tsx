import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { PASSAGE_REFERENCES_QUERY_KEY } from '@/features/reading/hooks/usePassageReferencesQuery';
import { toast } from '@/stores/toastStore';
import type { SourceWithMetadata } from '@/types/conversation';

import { usePassageCapture } from '../usePassageCapture';

const api = vi.hoisted(() => ({ createPassageReference: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const source = (overrides: Partial<SourceWithMetadata> = {}) => ({
  documentId: 'doc 1', chunkId: 'doc-1#3', filePath: '/library/silk-road.pdf', fileName: 'Silk Road.pdf', ...overrides,
} as SourceWithMetadata);

function setup(options: { source?: SourceWithMetadata; initialLocator?: { chunkId?: string } | null; resolvedLabel?: string } = {}) {
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const onClose = vi.fn();
  let location = '';
  const Probe = () => { const here = useLocation(); location = here.pathname + here.search; return null; };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}><MemoryRouter initialEntries={['/reader']}>{children}<Probe /></MemoryRouter></QueryClientProvider>
  );
  const hook = renderHook(() => usePassageCapture({
    source: options.source ?? source(), initialLocator: (options.initialLocator ?? null) as never, resolvedLabel: options.resolvedLabel, onClose,
  }), { wrapper });
  return { ...hook, onClose, invalidate, location: () => location };
}

beforeEach(() => {
  api.createPassageReference.mockReset().mockResolvedValue({ ok: true, data: { id: 'ref-1' } });
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
});
afterEach(() => vi.restoreAllMocks());

describe('usePassageCapture', () => {
  it('saves a trimmed passage as a reference with where it was found', async () => {
    const { result, invalidate } = setup({ initialLocator: { chunkId: 'doc-1#9' }, resolvedLabel: 'p. 12' });
    await act(() => result.current.handleReference('  Caravans crossed the Taklamakan.  '));
    expect(api.createPassageReference).toHaveBeenCalledWith({
      documentId: 'doc 1', chunkId: 'doc-1#9', filePath: '/library/silk-road.pdf', fileName: 'Silk Road.pdf', locator: 'p. 12',
      text: 'Caravans crossed the Taklamakan.',
    });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: PASSAGE_REFERENCES_QUERY_KEY });
    expect(toast.success).toHaveBeenCalledWith('Saved to references', { message: 'Lattice will use this in future answers.' });
  });

  it('falls back to the source chunk and its own location, and caps the text', async () => {
    const { result } = setup({ source: source({ pageNumber: 4 }) });
    await act(() => result.current.handleReference('x'.repeat(4100)));
    expect(api.createPassageReference).toHaveBeenCalledWith(expect.objectContaining({ chunkId: 'doc-1#3', locator: 'PDF p. 4', text: 'x'.repeat(4000) }));
  });

  it('sends no locator when the passage has no known location', async () => {
    const { result } = setup({ source: source({ chunkId: '' }) });
    await act(() => result.current.handleReference('Marco Polo'));
    expect(api.createPassageReference.mock.calls[0][0].locator).toBeUndefined();
  });

  it('says why a reference could not be saved', async () => {
    api.createPassageReference.mockResolvedValue({ ok: false, error: 'document was removed' });
    const { result, invalidate } = setup();
    await act(() => result.current.handleReference('Samarkand'));
    expect(toast.error).toHaveBeenCalledWith("Couldn't save this reference", { message: 'document was removed' });
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('ignores a blank passage for every verb', async () => {
    const { result, onClose } = setup();
    await act(() => result.current.handleReference('   '));
    await act(() => result.current.handleAddToJournal('\n'));
    act(() => result.current.handleAskAbout(' '));
    expect(api.createPassageReference).not.toHaveBeenCalled();
    expect(result.current.captureDraft).toBeNull();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('drafts a journal quote with attribution', async () => {
    const { result } = setup({ resolvedLabel: 'p. 7' });
    await act(() => result.current.handleAddToJournal('Silk moved west.\nGlass moved east.'));
    expect(result.current.captureDraft).toBe('> Silk moved west.\n> Glass moved east.\n— Silk Road.pdf, p. 7');
    act(() => result.current.setCaptureDraft(null));
    expect(result.current.captureDraft).toBeNull();
  });

  it('attributes a quote to the file alone when it has no location', async () => {
    const { result } = setup({ source: source({ chunkId: '' }) });
    await act(() => result.current.handleAddToJournal('Bukhara'));
    expect(result.current.captureDraft).toBe('> Bukhara\n— Silk Road.pdf');
  });

  it('closes the reader and asks about the passage in a new chat', () => {
    const { result, onClose, location } = setup();
    act(() => result.current.handleAskAbout(' Why did trade decline? '));
    expect(onClose).toHaveBeenCalled();
    expect(location()).toBe('/chat?new=1&documentId=doc%201&quote=Why%20did%20trade%20decline%3F');
  });
});
