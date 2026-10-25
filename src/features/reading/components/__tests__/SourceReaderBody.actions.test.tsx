import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';


import { SourceReaderBody } from '@/features/reading/components/SourceReaderBody';
import { useWebPageSnapshotStore } from '@/features/reading/snapshot/webPageSnapshotStore';
import VaultAPI from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';

const mocks = vi.hoisted(() => ({
  resolveSourcePath: vi.fn(),
  openSourceDocument: vi.fn(),
  openSourcePath: vi.fn(),
  revealSourcePath: vi.fn(),
  readFileContent: vi.fn(),
  createPassageReference: vi.fn(),
  openExternal: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  default: { readFileContent: mocks.readFileContent, createPassageReference: mocks.createPassageReference },
  VaultAPI: { readFileContent: mocks.readFileContent, createPassageReference: mocks.createPassageReference },
}));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: mocks.openExternal }));
vi.mock('@/features/reading/hooks/sourceFileActions', () => ({
  resolveSourcePath: mocks.resolveSourcePath,
  openSourceDocument: mocks.openSourceDocument,
  openSourcePath: mocks.openSourcePath,
  revealSourcePath: mocks.revealSourcePath,
}));
vi.mock('react-router', () => ({ useNavigate: () => mocks.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ activeConversationId: null, conversations: [], spaces: [], selectedSpaceId: null, loadConversationLinkedDocuments: vi.fn() }),
}));
vi.mock('@/features/reading', () => ({
  PassageHighlighter: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  SelectionToolbar: () => null,
  useTextSelection: () => null,
}));
vi.mock('@/features/reading/snapshot/api', () => ({
  getWebPageSnapshot: vi.fn().mockResolvedValue(null),
  captureWebPageSnapshot: vi.fn(),
  listWebPageSnapshots: vi.fn(),
}));
vi.mock('@/features/reading/components/WebArticleView', () => ({ WebArticleView: () => <div>web article</div> }));
vi.mock('@/features/reading/components/CitationRail', () => ({
  CitationRail: (props: { onPrevious?: () => void; onNext?: () => void; onReference: () => void; onAddToJournal: () => void; onAskAbout: () => void }) => (
    <div>
      {props.onPrevious && <button type="button" onClick={props.onPrevious}>Previous citation</button>}
      {props.onNext && <button type="button" onClick={props.onNext}>Next citation</button>}
      <button type="button" onClick={props.onReference}>Save reference</button>
      <button type="button" onClick={props.onAddToJournal}>Add to journal</button>
      <button type="button" onClick={props.onAskAbout}>Ask about it</button>
    </div>
  ),
}));
vi.mock('@/features/reading/components/JournalCapturePreview', () => ({
  JournalCapturePreview: ({ content, onOpenNote, onClose }: { content: string; onOpenNote: (id: string) => void; onClose: () => void }) => (
    <div role="dialog" aria-label="Journal capture">
      <pre>{content}</pre>
      <button type="button" onClick={() => onOpenNote('note 7')}>Open note</button>
      <button type="button" onClick={onClose}>Discard capture</button>
    </div>
  ),
}));
vi.mock('@/features/reading/components/viewers/TextViewer', () => ({
  TextViewer: ({ content }: { content: string }) => <div data-testid="file-content">{content}</div>,
}));

const DOCUMENT_ID = '123e4567-e89b-12d3-a456-426614174000';

const source = (overrides: Partial<SourceWithMetadata> = {}): SourceWithMetadata => ({
  documentId: DOCUMENT_ID, chunkId: 'chunk-1', fileName: 'harvest.txt', filePath: '/farm/harvest.txt', mimeType: 'text/plain',
  category: 'local file', content: 'Barley ripens in July.', excerpt: '', score: 1, fileSizeBytes: 100,
  modifiedAt: '2026-09-20T00:00:00.000Z', ...overrides,
});
const webSource = () => source({
  documentId: 'web:https%3A%2F%2Fexample.com%2Fgrain', fileName: 'Grain history', filePath: 'https://example.com/grain',
  mimeType: 'text/html', category: 'web',
});

function renderBody(current: SourceWithMetadata, extra: Partial<React.ComponentProps<typeof SourceReaderBody>> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <SourceReaderBody presentation="docked" source={current} onClose={onClose} {...extra} />
    </QueryClientProvider>,
  );
  return { onClose };
}

const toasts = () => useToastStore.getState().toasts.map(({ title, message }) => ({ title, message }));
const click = (name: string | RegExp) => fireEvent.click(screen.getByRole('button', { name }));

beforeEach(() => {
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [] });
  useWebPageSnapshotStore.setState({ mode: null });
  mocks.resolveSourcePath.mockResolvedValue({ ok: true, data: '/vault/harvest.txt' });
  mocks.readFileContent.mockResolvedValue({ ok: true, data: 'Barley ripens in July.' });
  mocks.openSourceDocument.mockResolvedValue({ ok: true, data: { action: 'opened_external' } });
  mocks.openSourcePath.mockResolvedValue({ ok: true, data: null });
  mocks.revealSourcePath.mockResolvedValue({ ok: true, data: null });
  mocks.openExternal.mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());

describe('SourceReaderBody footer actions', () => {
  it('opens a library document by its id and reports a failure', async () => {
    renderBody(source());
    click('Open file');
    await waitFor(() => expect(mocks.openSourceDocument).toHaveBeenCalledWith(DOCUMENT_ID, VaultAPI));
    expect(mocks.openSourcePath).not.toHaveBeenCalled();

    mocks.openSourceDocument.mockResolvedValue({ ok: false, error: 'no default app' });
    click('Open file');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't open file", message: 'no default app' }]));
  });

  it('opens the rendered copy of a document the app renders itself', async () => {
    mocks.openSourceDocument.mockResolvedValue({ ok: true, data: { action: 'render_internal', contentPath: '/cache/harvest.html' } });
    mocks.openSourcePath.mockResolvedValue({ ok: false, error: 'archive missing' });
    renderBody(source());
    click('Open file');
    await waitFor(() => expect(mocks.openSourcePath).toHaveBeenCalledWith('/cache/harvest.html', VaultAPI));
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't open archive", message: 'archive missing' }]));
  });

  it('opens a file with no library id by its path and reports a failure', async () => {
    mocks.openSourcePath.mockResolvedValue({ ok: false, error: 'moved' });
    renderBody(source({ documentId: 'loose-file' }));
    click('Open file');
    await waitFor(() => expect(mocks.openSourcePath).toHaveBeenCalledWith('/farm/harvest.txt', VaultAPI));
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't open file", message: 'moved' }]));
    expect(mocks.openSourceDocument).not.toHaveBeenCalled();
  });

  it('reveals a library document where it now lives', async () => {
    renderBody(source());
    await screen.findByTestId('file-content');
    click('Show in file manager');
    await waitFor(() => expect(mocks.revealSourcePath).toHaveBeenCalledWith('/vault/harvest.txt', VaultAPI));
    expect(toasts()).toEqual([]);
  });

  it('reports a document that cannot be located or revealed', async () => {
    renderBody(source());
    await screen.findByTestId('file-content');
    mocks.resolveSourcePath.mockResolvedValue({ ok: false, error: 'not in the library' });
    click('Show in file manager');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't locate file", message: 'not in the library' }]));

    useToastStore.setState({ toasts: [] });
    mocks.resolveSourcePath.mockResolvedValue({ ok: true, data: '/vault/harvest.txt' });
    mocks.revealSourcePath.mockResolvedValue({ ok: false, error: 'finder unavailable' });
    click('Show in file manager');
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't show in file manager", message: 'finder unavailable' }]));
  });

  it('reveals a loose file by its path', async () => {
    mocks.revealSourcePath.mockResolvedValue({ ok: false, error: 'gone' });
    renderBody(source({ documentId: 'loose-file' }));
    click('Show in file manager');
    await waitFor(() => expect(mocks.revealSourcePath).toHaveBeenCalledWith('/farm/harvest.txt', VaultAPI));
    await waitFor(() => expect(toasts()).toEqual([{ title: "Couldn't show in file manager", message: 'gone' }]));
  });

  it('opens a web source in the browser, falling back to a new window', async () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null);
    renderBody(webSource());
    expect(screen.queryByRole('button', { name: 'Show in file manager' })).not.toBeInTheDocument();
    click('Open URL');
    await waitFor(() => expect(mocks.openExternal).toHaveBeenCalledWith('https://example.com/grain'));
    expect(open).not.toHaveBeenCalled();

    mocks.openExternal.mockRejectedValue(new Error('shell blocked'));
    click('Open URL');
    await waitFor(() => expect(open).toHaveBeenCalledWith('https://example.com/grain', '_blank', 'noopener,noreferrer'));
    expect(toasts()).toEqual([{ title: "Couldn't open URL", message: 'shell blocked' }]);
  });
});

describe('SourceReaderBody citation rail', () => {
  const locator = { chunkId: 'chunk-1', text: 'Barley ripens in July.' } as PassageLocator;

  it('travels between citations', async () => {
    const onCitationIndexChange = vi.fn();
    const citations = [source(), source({ documentId: '223e4567-e89b-12d3-a456-426614174000', chunkId: 'chunk-2' })];
    renderBody(source(), { initialLocator: locator, citations, citationIndex: 0, onCitationIndexChange });
    click('Next citation');
    expect(onCitationIndexChange).toHaveBeenCalledWith(1);
    // Before the first citation there is nowhere further back to go.
    click('Previous citation');
    expect(onCitationIndexChange).toHaveBeenLastCalledWith(0);
  });

  it('quotes the cited passage into a journal draft and opens the saved note', async () => {
    const { onClose } = renderBody(source(), { initialLocator: locator });
    expect(screen.queryByRole('button', { name: 'Next citation' })).not.toBeInTheDocument();
    click('Add to journal');
    const preview = await screen.findByRole('dialog', { name: 'Journal capture' });
    expect(preview).toHaveTextContent('> Barley ripens in July.');
    click('Open note');
    expect(onClose).toHaveBeenCalled();
    expect(mocks.navigate).toHaveBeenCalledWith('/journals?noteId=note%207');
    expect(screen.queryByRole('dialog', { name: 'Journal capture' })).not.toBeInTheDocument();
  });

  it('discards a journal draft without leaving the reader', async () => {
    const { onClose } = renderBody(source(), { initialLocator: locator });
    click('Add to journal');
    await screen.findByRole('dialog', { name: 'Journal capture' });
    click('Discard capture');
    expect(screen.queryByRole('dialog', { name: 'Journal capture' })).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('saves the passage as a reference and asks about it in chat', async () => {
    mocks.createPassageReference.mockResolvedValue({ ok: true, data: {} });
    const { onClose } = renderBody(source(), { initialLocator: locator });
    click('Save reference');
    await waitFor(() => expect(mocks.createPassageReference).toHaveBeenCalledWith(expect.objectContaining({ text: 'Barley ripens in July.', chunkId: 'chunk-1' })));
    click('Ask about it');
    expect(onClose).toHaveBeenCalled();
    expect(mocks.navigate).toHaveBeenCalledWith(expect.stringContaining('/chat?new=1&documentId='));
  });
});
