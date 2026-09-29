import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import { SourceReaderBody } from '../SourceReaderBody';

const mocks = vi.hoisted(() => ({
  getFilePathById: vi.fn(),
  readFileContent: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  default: { getFilePathById: mocks.getFilePathById },
  VaultAPI: { readFileContent: mocks.readFileContent },
}));
vi.mock('react-router', () => ({ useNavigate: () => vi.fn() }));
vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({
      activeConversationId: null,
      conversations: [],
      spaces: [],
      selectedSpaceId: null,
      loadConversationLinkedDocuments: vi.fn(),
    }),
}));
vi.mock('@/components/Reading', () => ({
  PassageHighlighter: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  SelectionToolbar: () => null,
  useTextSelection: () => null,
}));
vi.mock('@/components/ContentViewer/renderers/HTMLViewer', () => ({ HTMLViewer: () => null }));
vi.mock('../WebArticleView', () => ({ WebArticleView: () => null }));
vi.mock('@/components/Chat/CitationRail', () => ({ CitationRail: () => null }));
vi.mock('@/components/Chat/JournalCapturePreview', () => ({ JournalCapturePreview: () => null }));
vi.mock('@/components/Chat/viewers/AudioViewer', () => ({
  AudioViewer: () => null,
  audioViewerPropsFromSource: () => ({}),
}));
vi.mock('@/components/Chat/viewers/ImageViewer', () => ({ ImageViewer: () => null }));
vi.mock('@/components/Chat/viewers/MarkdownViewer', () => ({
  MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div>,
}));
vi.mock('@/components/Chat/viewers/PDFViewer', () => ({ PDFViewer: () => null }));
vi.mock('@/components/Chat/viewers/TextViewer', () => ({
  TextViewer: ({ content }: { content: string }) => <div data-testid="file-content">{content}</div>,
}));

const DOCUMENT_ID = '123e4567-e89b-12d3-a456-426614174000';

function source(overrides: Partial<SourceWithMetadata> = {}): SourceWithMetadata {
  return {
    documentId: DOCUMENT_ID,
    chunkId: 'chunk-1',
    fileName: 'notes.txt',
    filePath: '/old/location/notes.txt',
    mimeType: 'text/plain',
    category: 'local file',
    content: 'Saved citation content',
    excerpt: 'Saved citation excerpt',
    score: 1,
    fileSizeBytes: 100,
    modifiedAt: '2026-09-20T00:00:00.000Z',
    ...overrides,
  };
}

function renderBody(currentSource: SourceWithMetadata) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SourceReaderBody presentation="docked" source={currentSource} onClose={() => undefined} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getFilePathById.mockImplementation(async (documentId: string) => ({
    ok: true,
    data: documentId.startsWith('223e') ? '/vault/current/second.txt' : '/vault/current/notes.txt',
  }));
  mocks.readFileContent.mockResolvedValue({ ok: true, data: 'Full file at its current path' });
});

describe('SourceReaderBody local citations', () => {
  it('resolves a stable document ID even when citation metadata has an absolute stale path', async () => {
    renderBody(source());

    expect(await screen.findByText('Full file at its current path')).toBeTruthy();
    expect(mocks.getFilePathById).toHaveBeenCalledWith(DOCUMENT_ID);
    expect(mocks.readFileContent).toHaveBeenCalledWith('/vault/current/notes.txt');
  });

  it('shows the persisted citation excerpt when the original file cannot be read', async () => {
    mocks.readFileContent.mockResolvedValue({ ok: false, error: 'file not found' });
    renderBody(source({
      excerpt: 'The stored words from the cited passage',
      content: 'A larger citation chunk',
    }));

    expect(await screen.findByText('The stored words from the cited passage')).toBeTruthy();
    expect(screen.getByText(/original file is unavailable/i)).toBeTruthy();
    expect(screen.getByText(/not a full file snapshot/i)).toBeTruthy();
    expect(screen.queryByText(/file changed since it was indexed/i)).toBeNull();
    expect(screen.queryByText(/couldn't load file/i)).toBeNull();
  });

  it('falls back to saved citation content when the excerpt is blank', async () => {
    mocks.readFileContent.mockResolvedValue({ ok: false, error: 'file not found' });
    renderBody(source({ excerpt: '  ', content: 'Saved citation content remains available' }));

    expect(await screen.findByText('Saved citation content remains available')).toBeTruthy();
    expect(screen.getByText(/not a full file snapshot/i)).toBeTruthy();
  });

  it('ignores a late read from the previous citation after switching sources', async () => {
    let finishFirstRead!: (value: { ok: true; data: string }) => void;
    mocks.readFileContent
      .mockImplementationOnce(() => new Promise((resolve) => { finishFirstRead = resolve; }))
      .mockResolvedValueOnce({ ok: true, data: 'Second source content' });
    const view = renderBody(source({ fileName: 'first.txt' }));
    await waitFor(() => expect(mocks.readFileContent).toHaveBeenCalledTimes(1));

    view.rerender(
      <QueryClientProvider client={new QueryClient()}>
        <SourceReaderBody
          presentation="docked"
          source={source({
            documentId: '223e4567-e89b-12d3-a456-426614174000',
            fileName: 'second.txt',
            filePath: '/old/location/second.txt',
          })}
          onClose={() => undefined}
        />
      </QueryClientProvider>,
    );

    expect(await screen.findByText('Second source content')).toBeTruthy();
    await act(async () => {
      finishFirstRead({ ok: true, data: 'Stale first source content' });
      await Promise.resolve();
    });
    expect(screen.queryByText('Stale first source content')).toBeNull();
    expect(screen.getByText('Second source content')).toBeTruthy();
  });

  it('ignores a late path lookup from the previous citation after switching sources', async () => {
    let finishFirstLookup!: (value: { ok: true; data: string }) => void;
    mocks.getFilePathById
      .mockImplementationOnce(() => new Promise((resolve) => { finishFirstLookup = resolve; }))
      .mockResolvedValueOnce({ ok: true, data: '/vault/current/second.txt' });
    const view = renderBody(source({ fileName: 'first.txt' }));
    await waitFor(() => expect(mocks.getFilePathById).toHaveBeenCalledTimes(1));

    view.rerender(
      <QueryClientProvider client={new QueryClient()}>
        <SourceReaderBody
          presentation="docked"
          source={source({
            documentId: '223e4567-e89b-12d3-a456-426614174000',
            fileName: 'second.txt',
            filePath: '/old/location/second.txt',
          })}
          onClose={() => undefined}
        />
      </QueryClientProvider>,
    );

    await waitFor(() => expect(mocks.readFileContent).toHaveBeenCalledWith('/vault/current/second.txt'));
    await act(async () => {
      finishFirstLookup({ ok: true, data: '/vault/stale/first.txt' });
      await Promise.resolve();
    });
    expect(mocks.readFileContent).toHaveBeenCalledTimes(1);
    expect(mocks.readFileContent).not.toHaveBeenCalledWith('/vault/stale/first.txt');
  });
});

describe('SourceReaderBody minimize affordance', () => {
  it('offers a minimize button when the surface can tuck it away to a pill', async () => {
    const onMinimize = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <SourceReaderBody
          presentation="docked"
          source={source()}
          onClose={() => undefined}
          onMinimize={onMinimize}
        />
      </QueryClientProvider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Minimize reader' }));
    expect(onMinimize).toHaveBeenCalledTimes(1);
  });

  it('leaves the minimize button out when no handler is provided', async () => {
    renderBody(source());
    await screen.findByText('Full file at its current path');

    expect(screen.queryByRole('button', { name: 'Minimize reader' })).toBeNull();
  });
});
