import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { WebPageSnapshotDto } from '@/lib/bindings';

import { useWebPageSnapshotStore } from '../webPageSnapshotStore';
import { WebPageSnapshotView } from '../WebPageSnapshotView';

type Handler = (_event: { payload: unknown }) => void;

const mocks = vi.hoisted(() => ({
  getWebPageSnapshot: vi.fn(),
  captureWebPageSnapshot: vi.fn(),
  listWebPageSnapshots: vi.fn(),
  openExternalUrl: vi.fn(),
  listeners: new Map<string, Set<(_event: { payload: unknown }) => void>>(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}`,
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(),
  listen: vi.fn(async (name: string, handler: Handler) => {
    const set = mocks.listeners.get(name) ?? new Set<Handler>();
    set.add(handler);
    mocks.listeners.set(name, set);
    return () => set.delete(handler);
  }),
}));
vi.mock('@/lib/bindings', () => ({
  commands: {
    getWebPageSnapshot: mocks.getWebPageSnapshot,
    captureWebPageSnapshot: mocks.captureWebPageSnapshot,
    listWebPageSnapshots: mocks.listWebPageSnapshots,
  },
}));
vi.mock('@/utils/openExternalUrl', () => ({ openExternalUrl: mocks.openExternalUrl }));

const CONVERSATION = 'conv-1';
const URL_CITED = 'https://www.example.com/2026/10/story';
const FRAME = 'lattice-snapshot://localhost/snap-1%2Findex.html';

function dto(overrides: Partial<WebPageSnapshotDto> = {}): WebPageSnapshotDto {
  return {
    id: 'snap-1',
    conversationId: CONVERSATION,
    requestedUrl: URL_CITED,
    finalUrl: null,
    normalizedUrl: URL_CITED,
    title: 'Story',
    status: 'ready',
    method: 'http',
    capturedAt: '2026-10-13T12:00:00Z',
    byteSize: 2_516_582,
    sha256: 'abc',
    resourceCount: 38,
    missingResourceCount: 2,
    partial: false,
    partialReasons: [],
    errorCode: null,
    errorMessage: null,
    faviconDataUrl: null,
    progressMessage: null,
    verified: true,
    ...overrides,
  };
}

const ok = <T,>(data: T) => ({ status: 'ok', data });

function fire(name: string, payload: unknown) {
  act(() => {
    mocks.listeners.get(name)?.forEach((handler) => handler({ payload }));
  });
}

function renderView(props: Partial<Parameters<typeof WebPageSnapshotView>[0]> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <WebPageSnapshotView conversationId={CONVERSATION} url={URL_CITED} {...props} />
    </QueryClientProvider>,
  );
}

async function frame(): Promise<HTMLIFrameElement> {
  return (await screen.findByTitle('Frozen copy of example.com')) as HTMLIFrameElement;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.clear();
  localStorage.clear();
  useWebPageSnapshotStore.setState({ mode: null, zoom: 1, clean: true });
  mocks.getWebPageSnapshot.mockResolvedValue(ok(dto()));
  mocks.listWebPageSnapshots.mockResolvedValue(ok([]));
  mocks.openExternalUrl.mockResolvedValue(true);
});

afterEach(() => {
  vi.useRealTimers();
});

describe('WebPageSnapshotView states', () => {
  it('offers a capture when there is no frozen copy, and shows its progress', async () => {
    mocks.getWebPageSnapshot.mockResolvedValue(ok(null));
    mocks.captureWebPageSnapshot.mockResolvedValue(
      ok(dto({ status: 'pending', progressMessage: '12 of 40 resources', capturedAt: null })),
    );
    renderView();

    expect(await screen.findByText('No frozen copy of this page yet.')).toBeTruthy();
    expect(screen.getByText('The text the answer used is still in Reader.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Capture now' }));

    expect(await screen.findByText('Capturing the page…')).toBeTruthy();
    expect(screen.getByText('12 of 40 resources')).toBeTruthy();
    expect(mocks.captureWebPageSnapshot).toHaveBeenCalledWith(CONVERSATION, URL_CITED);
  });

  it('follows the changed event from pending to ready, for this conversation only', async () => {
    mocks.getWebPageSnapshot.mockResolvedValue(ok(dto({ status: 'pending' })));
    renderView();
    await screen.findByText('Capturing the page…');

    fire('web-page-snapshot:changed', { conversationId: 'other', url: URL_CITED, snapshot: dto() });
    expect(screen.getByText('Capturing the page…')).toBeTruthy();

    fire('web-page-snapshot:changed', { conversationId: CONVERSATION, url: URL_CITED, snapshot: dto() });
    expect(await frame()).toBeTruthy();
    expect(mocks.getWebPageSnapshot).toHaveBeenCalledTimes(1);
  });

  it('explains a failure and offers to try again or open the live page', async () => {
    mocks.getWebPageSnapshot.mockResolvedValue(
      ok(dto({ status: 'failed', errorMessage: 'The site answered 403, which usually means a human check.' })),
    );
    mocks.captureWebPageSnapshot.mockResolvedValue(ok(dto({ status: 'pending' })));
    renderView();

    expect(await screen.findByText("Couldn't capture this page.")).toBeTruthy();
    expect(screen.getByText('The site answered 403, which usually means a human check.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Open live page' }));
    expect(mocks.openExternalUrl).toHaveBeenCalledWith(URL_CITED);
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(mocks.captureWebPageSnapshot).toHaveBeenCalledWith(CONVERSATION, URL_CITED));
  });

  it('renders a ready copy in an empty sandbox with the status line', async () => {
    renderView();
    const iframe = await frame();

    expect(iframe.getAttribute('sandbox')).toBe('');
    expect(iframe.outerHTML).not.toMatch(/allow-/);
    expect(iframe.getAttribute('referrerpolicy')).toBe('no-referrer');
    expect(iframe.getAttribute('src')).toBe(`${FRAME}?clean=1`);
    expect(
      screen.getByText('✓ Verified · 2.4 MB · 38 resources, 2 missing · captured over HTTP'),
    ).toBeTruthy();
    expect(screen.getByText(/^Frozen /)).toBeTruthy();
  });

  it('marks an incomplete copy in amber and a failed integrity check in red', async () => {
    mocks.getWebPageSnapshot.mockResolvedValue(ok(dto({ partial: true, partialReasons: ['size budget'] })));
    const { unmount } = renderView();
    const partial = await screen.findByText('· incomplete: size budget');
    expect(partial.className).toContain('warning');
    unmount();

    mocks.getWebPageSnapshot.mockResolvedValue(ok(dto({ verified: false })));
    renderView();
    const failed = await screen.findByText('✗ Integrity check failed');
    expect(failed.className).toContain('danger');
  });

  it('hides the status line in the compact presentation', async () => {
    renderView({ compact: true });
    await frame();
    expect(screen.queryByText(/✓ Verified/)).toBeNull();
  });
});

describe('WebPageSnapshotView controls', () => {
  it('toggles the clean layer on the frame URL and remembers it', async () => {
    renderView();
    await frame();
    fireEvent.click(screen.getByRole('button', { name: 'Clean' }));

    expect((await frame()).getAttribute('src')).toBe(FRAME);
    expect(localStorage.getItem('chat.reader.pageClean')).toBe('false');
  });

  it('zooms the frame with the buttons and the keyboard', async () => {
    renderView();
    const iframe = await frame();
    expect(iframe.style.transform).toBe('scale(1)');
    expect(iframe.style.transformOrigin).toBe('0 0');

    fireEvent.click(screen.getByRole('button', { name: 'Zoom in' }));
    expect((await frame()).style.transform).toBe('scale(1.1)');
    expect(localStorage.getItem('chat.reader.pageZoom')).toBe('1.1');

    const pane = screen.getByRole('group');
    fireEvent.keyDown(pane, { key: '-', metaKey: true });
    fireEvent.keyDown(pane, { key: '-', metaKey: true });
    expect((await frame()).style.transform).toBe('scale(0.9)');
    fireEvent.keyDown(pane, { key: '0', metaKey: true });
    expect((await frame()).style.transform).toBe('scale(1)');
  });

  it('puts the frame back when it loads a second document', async () => {
    renderView();
    const iframe = await frame();
    fireEvent.load(iframe);
    iframe.setAttribute('src', 'https://elsewhere.example/');
    fireEvent.load(iframe);

    expect(iframe.getAttribute('src')).toBe(`${FRAME}?clean=1`);
  });
});

describe('WebPageSnapshotView link bar', () => {
  it('appears for a click in this snapshot and opens the browser only on request', async () => {
    renderView();
    await frame();

    fire('web-page-snapshot:link', { snapshotId: 'snap-other', url: 'https://example.org/path' });
    expect(screen.queryByRole('status')).toBeNull();

    fire('web-page-snapshot:link', { snapshotId: 'snap-1', url: 'https://example.org/path' });
    const bar = await screen.findByRole('status');
    expect(bar.textContent).toContain('This page links to example.org/path');
    expect(mocks.openExternalUrl).not.toHaveBeenCalled();
    await waitFor(() => expect(mocks.listWebPageSnapshots).toHaveBeenCalledWith(CONVERSATION));
    expect(screen.queryByRole('button', { name: 'Open frozen copy' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Open in browser' }));
    expect(mocks.openExternalUrl).toHaveBeenCalledWith('https://example.org/path');
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('offers and opens another frozen copy of the clicked page', async () => {
    mocks.listWebPageSnapshots.mockResolvedValue(
      ok([
        dto(),
        dto({ id: 'snap-2', requestedUrl: 'https://example.org/path', normalizedUrl: 'https://example.org/path' }),
      ]),
    );
    renderView();
    await frame();

    fire('web-page-snapshot:link', { snapshotId: 'snap-1', url: 'https://EXAMPLE.org/path#part' });
    fireEvent.click(await screen.findByRole('button', { name: 'Open frozen copy' }));

    const switched = (await screen.findByTitle('Frozen copy of example.org')) as HTMLIFrameElement;
    expect(switched.getAttribute('src')).toBe('lattice-snapshot://localhost/snap-2%2Findex.html?clean=1');
  });

  it('goes away by itself after twelve seconds', async () => {
    renderView();
    await frame();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    fire('web-page-snapshot:link', { snapshotId: 'snap-1', url: 'https://example.org/path' });
    expect(screen.getByRole('status')).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(11_000);
    });
    expect(screen.getByRole('status')).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(screen.queryByRole('status')).toBeNull();
  });
});
