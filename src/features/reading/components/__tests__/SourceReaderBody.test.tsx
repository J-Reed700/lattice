import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';


import { Dialog, DialogContent } from '@/components/ui/dialog';
import { SourceReaderBody } from '@/features/reading/components/SourceReaderBody';
import { captureWebPageSnapshot } from '@/features/reading/snapshot/api';
import { useWebPageSnapshotStore } from '@/features/reading/snapshot/webPageSnapshotStore';
import VaultAPI from '@/lib/api';
import type { SourceWithMetadata } from '@/types/conversation';


const mocks = vi.hoisted(() => ({
  resolveSourcePath: vi.fn(),
  readFileContent: vi.fn(),
  savedWebArticle: vi.fn(),
  getWebPageSnapshot: vi.fn(),
  activeConversationId: null as string | null,
}));

vi.mock('@/lib/api', () => ({
  default: { readFileContent: mocks.readFileContent, savedWebArticle: mocks.savedWebArticle },
  VaultAPI: { readFileContent: mocks.readFileContent, savedWebArticle: mocks.savedWebArticle },
}));
vi.mock('@/features/reading/hooks/sourceFileActions', () => ({
  resolveSourcePath: mocks.resolveSourcePath,
  openSourceDocument: vi.fn(),
  openSourcePath: vi.fn(),
  revealSourcePath: vi.fn(),
}));
vi.mock('react-router', () => ({ useNavigate: () => vi.fn() }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({
      activeConversationId: mocks.activeConversationId,
      conversations: [],
      spaces: [],
      selectedSpaceId: null,
      loadConversationLinkedDocuments: vi.fn(),
    }),
}));
vi.mock('@/features/reading', () => ({
  PassageHighlighter: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  SelectionToolbar: () => null,
  useTextSelection: () => null,
}));
vi.mock('@/features/reading/components/viewers/HTMLViewer', () => ({
  HTMLViewer: ({ article, htmlPath }: { article?: { pagePath: string | null }; htmlPath?: string }) => (
    <div data-testid="html-viewer" data-page={article?.pagePath ?? htmlPath ?? ''} />
  ),
}));
vi.mock('@/features/reading/components/viewers/DocxViewer', () => ({
  DocxViewer: ({ filePath, title }: { filePath: string; title?: string }) => (
    <div data-testid="docx-viewer" data-path={filePath}>{title}</div>
  ),
}));
vi.mock('@/features/reading/components/WebArticleView', () => ({
  WebArticleView: ({ source }: { source: SourceWithMetadata }) => (
    <div data-testid="web-article" data-text={source.webSnapshot?.text ?? ''}>{source.fileName}</div>
  ),
}));
vi.mock('@/features/reading/snapshot/api', () => ({
  getWebPageSnapshot: mocks.getWebPageSnapshot,
  captureWebPageSnapshot: vi.fn(),
  listWebPageSnapshots: vi.fn(),
}));
vi.mock('@/features/reading/snapshot/WebPageSnapshotView', () => ({
  WebPageSnapshotView: ({ toggle }: { toggle?: React.ReactNode }) => <div data-testid="page-view">{toggle}</div>,
}));
vi.mock('@/features/reading/components/CitationRail', () => ({ CitationRail: () => null }));
vi.mock('@/features/reading/components/JournalCapturePreview', () => ({ JournalCapturePreview: () => null }));
vi.mock('@/features/reading/components/viewers/AudioViewer', () => ({
  AudioViewer: () => null,
  audioViewerPropsFromSource: () => ({}),
}));
vi.mock('@/features/reading/components/viewers/ImageViewer', () => ({ ImageViewer: () => null }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({
  MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div>,
}));
vi.mock('@/features/reading/components/viewers/PDFViewer', () => ({ PDFViewer: () => null }));
vi.mock('@/features/reading/components/viewers/TextViewer', () => ({
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
  mocks.activeConversationId = null;
  mocks.getWebPageSnapshot.mockResolvedValue(null);
  useWebPageSnapshotStore.setState({ mode: null });
  mocks.resolveSourcePath.mockImplementation(async (documentId: string) => ({
    ok: true,
    data: documentId.startsWith('223e') ? '/vault/current/second.txt' : '/vault/current/notes.txt',
  }));
  mocks.readFileContent.mockResolvedValue({ ok: true, data: 'Full file at its current path' });
  mocks.savedWebArticle.mockResolvedValue({ ok: true, data: null });
});

describe('SourceReaderBody local citations', () => {
  it('resolves a stable document ID even when citation metadata has an absolute stale path', async () => {
    renderBody(source());

    expect(await screen.findByText('Full file at its current path')).toBeTruthy();
    expect(mocks.resolveSourcePath).toHaveBeenCalledWith(DOCUMENT_ID, VaultAPI);
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
    mocks.resolveSourcePath
      .mockImplementationOnce(() => new Promise((resolve) => { finishFirstLookup = resolve; }))
      .mockResolvedValueOnce({ ok: true, data: '/vault/current/second.txt' });
    const view = renderBody(source({ fileName: 'first.txt' }));
    await waitFor(() => expect(mocks.resolveSourcePath).toHaveBeenCalledTimes(1));

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

describe('SourceReaderBody course passages', () => {
  it('reads a course passage from its citation, with no file to open', async () => {
    renderBody(source({
      documentId: 'course:version-1',
      filePath: '',
      category: 'Course source',
      content: 'The whole passage of the course source.',
      excerpt: 'The whole passage…',
    }));

    expect((await screen.findByTestId('file-content')).textContent).toBe('The whole passage of the course source.');
    expect(mocks.readFileContent).not.toHaveBeenCalled();
    expect(mocks.resolveSourcePath).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Open file' })).toBeNull();
    expect(screen.queryByText(/Show in|Reveal in/)).toBeNull();
  });

  it('reads a saved web version as an article with no Page view and no live capture', async () => {
    mocks.activeConversationId = 'conv-1';
    renderBody(source({
      documentId: 'course:version-1',
      filePath: '',
      fileName: 'Saved story',
      mimeType: 'text/html',
      category: 'web',
      content: '',
      webSnapshot: {
        url: 'https://example.com/story',
        title: 'Saved story',
        text: 'The page as it was saved.',
        fetchedAt: null,
        truncated: false,
      },
    }));

    const article = await screen.findByTestId('web-article');
    expect(article.getAttribute('data-text')).toBe('The page as it was saved.');
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.queryByRole('tab', { name: 'Page' })).toBeNull();
    expect(screen.queryByTestId('page-view')).toBeNull();
    expect(mocks.getWebPageSnapshot).not.toHaveBeenCalled();
    expect(captureWebPageSnapshot).not.toHaveBeenCalled();
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

describe('SourceReaderBody Reader | Page toggle', () => {
  const webSource = (overrides: Partial<SourceWithMetadata> = {}) =>
    source({
      documentId: 'web:https%3A%2F%2Fexample.com%2Fstory',
      fileName: 'Example story',
      filePath: 'https://example.com/story',
      mimeType: 'text/html',
      category: 'web',
      ...overrides,
    });

  it('shows the toggle for a web citation, on Reader when there is no frozen copy', async () => {
    mocks.activeConversationId = 'conv-1';
    renderBody(webSource());

    const reader = await screen.findByRole('tab', { name: 'Reader' });
    expect(screen.getByRole('tablist')).toBeTruthy();
    expect(reader.getAttribute('aria-selected')).toBe('true');
    expect(screen.queryByTestId('page-view')).toBeNull();
    expect(mocks.getWebPageSnapshot).toHaveBeenCalledWith('conv-1', 'https://example.com/story');
  });

  it('opens on Page when a ready frozen copy exists, and keeps the choice made', async () => {
    mocks.activeConversationId = 'conv-1';
    mocks.getWebPageSnapshot.mockResolvedValue({ id: 'snap-1', status: 'ready' });
    renderBody(webSource());

    expect(await screen.findByTestId('page-view')).toBeTruthy();
    expect(screen.getByRole('tab', { name: 'Page' }).getAttribute('aria-selected')).toBe('true');

    fireEvent.click(screen.getByRole('tab', { name: 'Reader' }));
    expect(screen.queryByTestId('page-view')).toBeNull();
    expect(useWebPageSnapshotStore.getState().mode).toBe('reader');
  });

  it('leaves the toggle out for a local file', async () => {
    mocks.activeConversationId = 'conv-1';
    renderBody(source());
    await screen.findByText('Full file at its current path');

    expect(screen.queryByRole('tablist')).toBeNull();
    expect(mocks.getWebPageSnapshot).not.toHaveBeenCalled();
  });

  it('renders DocxViewer and does not read binary docx as utf-8 text', async () => {
    renderBody(source({
      documentId: undefined,
      fileName: 'document.docx',
      filePath: '/vault/document.docx',
      mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    }));

    const docx = await screen.findByTestId('docx-viewer');
    expect(docx).toBeTruthy();
    expect(docx.getAttribute('data-path')).toBe('/vault/document.docx');
    expect(mocks.readFileContent).not.toHaveBeenCalled();
  });

  it('queries snapshot with source.conversationId even without active conversation in store', async () => {
    mocks.activeConversationId = null;
    renderBody(webSource({ conversationId: 'persisted-conv-99' }));

    await screen.findByRole('tab', { name: 'Reader' });
    expect(mocks.getWebPageSnapshot).toHaveBeenCalledWith('persisted-conv-99', 'https://example.com/story');
  });

  it('enriches web source with readerText from snapshot when webSnapshot text is missing', async () => {
    mocks.activeConversationId = 'conv-1';
    mocks.getWebPageSnapshot.mockResolvedValue({
      id: 'snap-1',
      status: 'pending',
      readerText: 'Full immutable captured text from web source repository',
    });

    renderBody(source({
      documentId: 'web:https%3A%2F%2Fexample.com%2Fstory',
      fileName: 'Example story',
      filePath: 'https://example.com/story',
      mimeType: 'text/html',
      category: 'web',
      content: 'Short excerpt',
      excerpt: 'Short excerpt',
    }));

    const article = await screen.findByTestId('web-article');
    expect(article.getAttribute('data-text')).toBe('Full immutable captured text from web source repository');
  });
});

describe('SourceReaderBody saved web articles', () => {
  const savedArticle = (folder: string) => ({
    textPath: `${folder}article.md`,
    pagePath: `${folder}page.html`,
    title: 'A saved piece',
    url: 'https://example.com/a',
    links: [],
  });
  const articleSource = (filePath: string) =>
    source({
      documentId: undefined,
      fileName: 'article.md',
      filePath,
      mimeType: 'text/markdown',
      category: 'local file',
    });

  it('shows the saved page of an article at a Windows path, as the archive says', async () => {
    const folder = 'C:\\Users\\me\\.lattice\\web-archive\\example.com\\a-1\\';
    mocks.savedWebArticle.mockResolvedValue({ ok: true, data: savedArticle(folder) });
    renderBody(articleSource(`${folder}article.md`));

    const viewer = await screen.findByTestId('html-viewer');
    expect(viewer.getAttribute('data-page')).toBe(`${folder}page.html`);
    expect(mocks.savedWebArticle).toHaveBeenCalledWith(`${folder}article.md`);
    expect(mocks.readFileContent).not.toHaveBeenCalled();
  });

  it('shows the saved page of an article in a custom data folder', async () => {
    const folder = '/data/lattice/web-archive/x/';
    mocks.savedWebArticle.mockResolvedValue({ ok: true, data: savedArticle(folder) });
    renderBody(articleSource(`${folder}article.md`));

    const viewer = await screen.findByTestId('html-viewer');
    expect(viewer.getAttribute('data-page')).toBe(`${folder}page.html`);
    expect(mocks.readFileContent).not.toHaveBeenCalled();
  });

  it('reads a file the archive does not claim as plain markdown, whatever its path says', async () => {
    mocks.savedWebArticle.mockResolvedValue({ ok: true, data: null });
    const path = '/Users/me/.lattice/web-archive/id/article.md';
    renderBody(articleSource(path));

    expect(await screen.findByText('Full file at its current path')).toBeTruthy();
    expect(mocks.readFileContent).toHaveBeenCalledWith(path);
    expect(screen.queryByTestId('html-viewer')).toBeNull();
  });

  it('reads nothing while the archive has not answered, and shows the loading state', async () => {
    mocks.savedWebArticle.mockReturnValue(new Promise(() => undefined));
    const { container } = renderBody(articleSource('/data/notes/article.md'));

    await waitFor(() => expect(mocks.savedWebArticle).toHaveBeenCalledWith('/data/notes/article.md'));
    await act(async () => {
      await Promise.resolve();
    });
    expect(mocks.readFileContent).not.toHaveBeenCalled();
    expect(container.querySelector('.animate-spin')).not.toBeNull();
    expect(screen.queryByTestId('html-viewer')).toBeNull();
  });

  it("asks the archive about the document's current path when the cited path is stale", async () => {
    const folder = '/vault/current/web-archive/example.com/a-1/';
    mocks.resolveSourcePath.mockResolvedValue({ ok: true, data: `${folder}article.md` });
    mocks.savedWebArticle.mockImplementation(async (path: string) => ({
      ok: true,
      data: path === `${folder}article.md` ? savedArticle(folder) : null,
    }));
    renderBody(
      source({
        fileName: 'article.md',
        filePath: '/old/home/web-archive/example.com/a-1/article.md',
        mimeType: 'text/markdown',
        category: 'local file',
      }),
    );

    const viewer = await screen.findByTestId('html-viewer');
    expect(viewer.getAttribute('data-page')).toBe(`${folder}page.html`);
    expect(mocks.savedWebArticle).toHaveBeenCalledWith(`${folder}article.md`);
    expect(mocks.savedWebArticle).not.toHaveBeenCalledWith(
      '/old/home/web-archive/example.com/a-1/article.md',
    );
  });

  it('ignores stale activeConversationId when standalone dialog views open a web source without explicit conversationId', async () => {
    mocks.activeConversationId = 'stale-chat-1';
    mocks.getWebPageSnapshot.mockResolvedValue({ ok: true, data: null });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <Dialog open onOpenChange={() => undefined}>
          <DialogContent>
            <SourceReaderBody
              presentation="dialog"
              source={source({
                documentId: 'web:https://example.com/standalone',
                filePath: 'https://example.com/standalone',
                fileName: 'Standalone page',
                mimeType: 'text/html',
                category: 'Web Page',
                content: 'Snippet',
              })}
              initialLocator={{ text: 'Snippet' }}
              onClose={vi.fn()}
            />
          </DialogContent>
        </Dialog>
      </QueryClientProvider>,
    );

    await waitFor(() =>
      expect(mocks.getWebPageSnapshot).toHaveBeenCalledWith('', 'https://example.com/standalone'),
    );
  });

  it('uses explicit conversationId on source or prop even when activeConversationId differs (e.g. tangents)', async () => {
    mocks.activeConversationId = 'main-chat-1';
    mocks.getWebPageSnapshot.mockResolvedValue({ ok: true, data: null });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <Dialog open onOpenChange={() => undefined}>
          <DialogContent>
            <SourceReaderBody
              presentation="docked"
              conversationId="tangent-9"
              source={source({
                documentId: 'web:https://example.com/tangent-page',
                conversationId: 'tangent-9',
                filePath: 'https://example.com/tangent-page',
                fileName: 'Tangent page',
                mimeType: 'text/html',
                category: 'Web Page',
                content: 'Snippet',
              })}
              initialLocator={{ text: 'Snippet' }}
              onClose={vi.fn()}
            />
          </DialogContent>
        </Dialog>
      </QueryClientProvider>,
    );

    await waitFor(() =>
      expect(mocks.getWebPageSnapshot).toHaveBeenCalledWith('tangent-9', 'https://example.com/tangent-page'),
    );
  });

  it('keeps local vault file viewers when anchoredBy is attached without a web snapshot', async () => {
    mocks.resolveSourcePath.mockResolvedValue({ ok: true, data: '/vault/paper.txt' });
    mocks.readFileContent.mockResolvedValue({
      ok: true,
      data: 'Full local vault text.\nAnchored evidence sentence.',
    });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <Dialog open onOpenChange={() => undefined}>
          <DialogContent>
            <SourceReaderBody
              presentation="docked"
              source={source({
                anchoredBy: [
                  {
                    occurrence: 0,
                    relation: 'backs',
                    anchors: [{ exact: 'Anchored evidence sentence.' }],
                  },
                ],
              })}
              occurrence={0}
              initialLocator={{ text: 'Anchored evidence sentence.' }}
              onClose={vi.fn()}
            />
          </DialogContent>
        </Dialog>
      </QueryClientProvider>,
    );

    const fileView = await screen.findByTestId('file-content');
    expect(fileView).toHaveTextContent('Anchored evidence sentence.');
    expect(screen.queryByTestId('web-article')).not.toBeInTheDocument();
  });

  it('renders anchored web snapshot sources through WebArticleView', async () => {
    mocks.getWebPageSnapshot.mockResolvedValue({ ok: true, data: null });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <Dialog open onOpenChange={() => undefined}>
          <DialogContent>
            <SourceReaderBody
              presentation="docked"
              source={source({
                documentId: 'web:https://example.org/guide',
                filePath: 'https://example.org/guide',
                fileName: 'Operations Guide',
                mimeType: 'text/html',
                category: 'Web article',
                webSnapshot: {
                  url: 'https://example.org/guide',
                  title: 'Operations Guide',
                  text: '# Guide\n\nHold the mitigation for 20 seconds.',
                  fetchedAt: null,
                  truncated: false,
                },
                anchoredBy: [
                  {
                    occurrence: 0,
                    relation: 'quotes',
                    anchors: [{ exact: 'Hold the mitigation for 20 seconds.' }],
                  },
                ],
              })}
              occurrence={0}
              initialLocator={{ text: 'Hold the mitigation for 20 seconds.' }}
              onClose={vi.fn()}
            />
          </DialogContent>
        </Dialog>
      </QueryClientProvider>,
    );

    const webView = await screen.findByTestId('web-article');
    expect(webView).toHaveAttribute('data-text', '# Guide\n\nHold the mitigation for 20 seconds.');
    expect(screen.queryByTestId('file-content')).not.toBeInTheDocument();
  });
});
