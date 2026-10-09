import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, renderHook, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';


import { SOURCE_PAGE_SIZE_KEY, SourcesPanel } from '@/features/learning/sources/SourcesPanel';
import { learningSourcesKey, useAddLearningWebSource } from '@/features/learning/sources/useLearningSources';
import { learningProgramKey, LEARNING_PROGRAMS_KEY } from '@/features/learning/workspace/useLearningStudio';
import type { LearningSourceVersionDto, LearningSourceWorkspaceDto } from '@/lib/bindings';
import type { DocumentMetadata } from '@/types/fileBrowser';

const mocks = vi.hoisted(() => ({
  workspace: vi.fn(), version: vi.fn(), search: vi.fn(), addWeb: vi.fn(), addDocument: vi.fn(), addText: vi.fn(),
  refresh: vi.fn(), adopt: vi.fn(), policy: vi.fn(), documents: vi.fn(), refreshFiles: vi.fn(),
}));

vi.mock('@/lib/api', () => ({ default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: mocks.documents().map((doc: { id: string; fileName: string }) => ({ documentId: doc.id, fileName: doc.fileName, category: null, modifiedAt: null })) }),
  getLearningSourceWorkspace: mocks.workspace,
  getLearningSourceVersion: mocks.version,
  searchLearningSources: mocks.search,
  addLearningWebSource: mocks.addWeb,
  addLearningDocumentSource: mocks.addDocument,
  addLearningTextSource: mocks.addText,
  refreshLearningSource: mocks.refresh,
  adoptLearningSourceVersion: mocks.adopt,
  updateLearningSourcePolicy: mocks.policy,
  getLearningProgram: vi.fn(), listLearningPrograms: vi.fn(),
} }));


const activeText = {
  id: 'version-1', versionNumber: 1, title: 'Field notes', publisher: 'Open Archive', resolvedUrl: 'https://archive.example/field',
  excerpt: 'A saved passage about careful observation.', contentSha256: 'sha256-a', wordCount: 9, truncated: false,
  extractionVersion: 'reader-1', acquiredAt: 1_790_000_000_000,
};
const pendingText = { ...activeText, id: 'version-2', versionNumber: 2, title: 'Field notes revised', excerpt: 'A revised passage about careful observation.', contentSha256: 'sha256-b', acquiredAt: 1_790_100_000_000 };
const source = {
  id: 'source-1', kind: 'web' as const, origin: 'web', requestedUrl: 'https://archive.example/field', freshnessPolicy: 'manual' as const,
  activeVersionId: 'version-1', pendingVersionId: 'version-2', revision: 4, deletedAt: null, deletionReason: null, activeVersion: activeText, pendingVersion: pendingText,
  versions: [pendingText, activeText], latestCheck: { operationId: 'check-1', status: 'update_available' as const, checkedAt: 1_790_100_000_000, activeDigest: 'sha256-a', pendingVersionId: 'version-2', message: null },
  checks: [], createdAt: 1_790_000_000_000, updatedAt: 1_790_100_000_000,
};
const workspace: LearningSourceWorkspaceDto = { programId: 'program-1', sources: [source] };
const version: LearningSourceVersionDto = {
  sourceId: 'source-1', version: activeText,
  fullText: 'A saved passage about careful observation.\n\nA full second paragraph from the captured text.',
  usage: [{ lessonId: 'lesson-1', lessonTitle: 'Observe closely', referenceKind: 'lesson_block', referenceTitle: 'Evidence' }],
};
const doc: DocumentMetadata = { id: 'doc-1', fileName: 'Field guide.pdf', filePath: '/Field guide.pdf', fileType: 'pdf', category: 'document', language: 'en', modifiedAt: '', indexedAt: '', wordCount: 420 };
const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });

function renderSources(initial = workspace, openedVersion = version) {
  mocks.workspace.mockResolvedValue(ok(initial));
  mocks.version.mockResolvedValue(ok(openedVersion));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><SourcesPanel programId="program-1" /></QueryClientProvider>);
}

describe('Learning Studio Sources workspace', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.documents.mockReturnValue([doc]);
    mocks.search.mockResolvedValue(ok([]));
  });

  it('opens an exact saved version with immutable provenance, usage, and version history', async () => {
    renderSources();
    expect(await screen.findByText('A saved passage about careful observation.')).toBeVisible();
    expect(await screen.findByText(/sha256-a/)).toBeVisible();
    expect(screen.getByText('Observe closely')).toBeVisible();
    expect(screen.getByText(/Version 1 · captured/)).toBeVisible();
    expect(screen.getByText('Update available · saved edition stays active')).toBeVisible();
    await userEvent.setup().selectOptions(screen.getByRole('combobox', { name: 'Source version history' }), 'version-2');
    await waitFor(() => expect(mocks.version).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-2' }));
  });

  it('renders legacy web snapshots as a formatted reader and keeps the exact capture available', async () => {
    const saved = {
      ...version,
      version: { ...version.version, wordCount: 24, extractionVersion: 'web_reference_v1' },
      fullText: '# Build cache\n\nCargo stores output in the target directory. By default,\nthis is inside the workspace.\n\n | Directory | Description\n\n | target/debug/ | Development output',
    };
    const { container } = renderSources(workspace, saved);

    expect(await screen.findByRole('heading', { name: 'Build cache', level: 1 })).toBeVisible();
    expect(screen.getByText('Cargo stores output in the target directory. By default, this is inside the workspace.')).toBeVisible();
    expect(container.querySelector('table')).toBeInTheDocument();

    await userEvent.setup().click(screen.getByRole('button', { name: 'Exact capture' }));
    expect(screen.getByText(/immutable text used for evidence/)).toBeVisible();
    expect(container.querySelector('pre')).toHaveTextContent('By default, this is inside the workspace.');
  });

  it('pages through a long library, remembers how many to show, and turns to a search match', async () => {
    const user = userEvent.setup();
    localStorage.removeItem(SOURCE_PAGE_SIZE_KEY);
    const many = Array.from({ length: 23 }, (_, index) => {
      const saved = { ...activeText, id: `version-${index + 10}`, title: `Archive ${index + 1}` };
      return { ...source, id: `source-${index + 10}`, activeVersionId: saved.id, activeVersion: saved, versions: [saved], pendingVersionId: index === 21 ? 'version-pending' : null, pendingVersion: null };
    });
    mocks.search.mockResolvedValue(ok(many.map((item, index) => ({ sourceId: item.id, versionId: item.activeVersionId, title: `Archive ${index + 1}`, excerpt: `match ${index + 1}` }))));
    renderSources({ programId: 'program-1', sources: many });

    const list = await screen.findByLabelText('Source list');
    const titles = () => within(list).getAllByRole('button').map((row) => row.textContent?.replace(/(Update available|Stored for offline reading)$/, ''));
    await waitFor(() => expect(titles()).toHaveLength(10));
    expect(titles()[0]).toBe('Archive 1');
    expect(screen.getByText('1–10 of 23')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Previous page' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Next page' }));
    expect(titles()[0]).toBe('Archive 11');
    await user.click(screen.getByRole('button', { name: 'Last page' }));
    expect(screen.getByText('21–23 of 23')).toBeVisible();
    expect(titles()).toEqual(['Archive 21', 'Archive 22', 'Archive 23']);
    expect(screen.getByRole('button', { name: 'Next page' })).toBeDisabled();

    // A narrower filter starts again at the first page.
    await user.click(screen.getByRole('button', { name: 'Updates' }));
    expect(titles()).toEqual(['Archive 22']);
    expect(screen.queryByRole('navigation', { name: 'Source pages' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'All' }));
    expect(titles()[0]).toBe('Archive 1');

    await user.selectOptions(screen.getByRole('combobox', { name: 'Sources per page' }), '25');
    expect(titles()).toHaveLength(23);
    expect(localStorage.getItem(SOURCE_PAGE_SIZE_KEY)).toBe('25');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Sources per page' }), '10');

    // Opening a match from the search results turns the list to the page that holds it.
    await user.type(screen.getByRole('textbox', { name: 'Search saved source text' }), 'archive');
    await user.click(await screen.findByRole('button', { name: /^Archive 22.*match 22$/ }));
    expect(titles()).toEqual(['Archive 21', 'Archive 22', 'Archive 23']);
    expect(within(list).getByRole('button', { name: /Archive 22/ })).toHaveAttribute('aria-current', 'true');
  });

  it('supports paste text and retries a lost response with the exact same operation and version ids', async () => {
    const user = userEvent.setup();
    mocks.addText.mockResolvedValueOnce(fail('Connection interrupted')).mockResolvedValueOnce(ok({ ...workspace, sources: [] }));
    renderSources({ programId: 'program-1', sources: [] });
    await user.click(await screen.findByRole('button', { name: 'Add material' }));
    await user.click(screen.getByRole('button', { name: 'Paste text' }));
    expect(screen.getByLabelText('Title')).toHaveFocus();
    await user.type(screen.getByLabelText('Title'), 'Personal field notes');
    await user.type(screen.getByLabelText('Saved text'), 'A learner-authored saved excerpt.');
    await user.click(screen.getByRole('button', { name: 'Save source' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    const firstRequest = mocks.addText.mock.calls[0][0];
    expect(firstRequest.operationId).toMatch(/^[0-9a-f]{8}-[0-9a-f-]{27}$/i);
    await user.click(screen.getByRole('button', { name: 'Retry save source' }));
    await waitFor(() => expect(mocks.addText).toHaveBeenCalledTimes(2));
    expect(mocks.addText.mock.calls[1][0]).toEqual(firstRequest);
  });

  it('adds a web page with an explicit policy and safely shows only canonical external links', async () => {
    const user = userEvent.setup();
    mocks.addWeb.mockResolvedValue(ok(workspace));
    renderSources({ programId: 'program-1', sources: [] });
    await user.click(await screen.findByRole('button', { name: 'Add material' }));
    await user.type(screen.getByLabelText('Web address'), 'https://example.org/article');
    await user.selectOptions(screen.getByLabelText('Freshness policy'), 'before_use');
    await user.click(screen.getByRole('button', { name: 'Save source' }));
    await waitFor(() => expect(mocks.addWeb).toHaveBeenCalledTimes(1));
    expect(mocks.addWeb.mock.calls[0][0]).toMatchObject({ url: 'https://example.org/article', freshnessPolicy: 'before_use' });
    const external = await screen.findByRole('link', { name: 'Open reference' });
    expect(external).toHaveAttribute('href', 'https://archive.example/field');
    expect(external).toHaveAttribute('target', '_blank');
  });

  it('refreshes the source workspace and program citation cache after a source write', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    mocks.addWeb.mockResolvedValue(ok(workspace));
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
    const { result } = renderHook(() => useAddLearningWebSource(), { wrapper });
    await result.current.mutateAsync({ operationId: 'op-1', sourceId: 'source-2', versionId: 'version-3', programId: 'program-1', url: 'https://example.org', freshnessPolicy: 'manual' });
    expect(client.getQueryData(learningSourcesKey('program-1'))).toEqual(workspace);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: learningProgramKey('program-1') });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: LEARNING_PROGRAMS_KEY });
  });

  it('adds an indexed Library document from the existing Library query', async () => {
    const user = userEvent.setup();
    mocks.addDocument.mockResolvedValue(ok(workspace));
    renderSources({ programId: 'program-1', sources: [] });
    await user.click(await screen.findByRole('button', { name: 'Add material' }));
    await user.click(screen.getByRole('button', { name: 'Library document' }));
    await user.click(await screen.findByRole('checkbox'));
    await user.click(screen.getByRole('button', { name: 'Save source' }));
    await waitFor(() => expect(mocks.addDocument).toHaveBeenCalledWith(expect.objectContaining({ documentId: 'doc-1', programId: 'program-1' })));
  });

  it('searches only stored text, opens the exact matching historical version, and changes freshness policy', async () => {
    const user = userEvent.setup();
    mocks.search.mockResolvedValue(ok([{ sourceId: 'source-1', versionId: 'version-1', title: 'Field notes', excerpt: '…careful observation…' }]));
    mocks.policy.mockResolvedValue(ok(workspace));
    renderSources();
    await user.type(await screen.findByRole('textbox', { name: 'Search saved source text' }), 'observation');
    await waitFor(() => expect(mocks.search).toHaveBeenCalledWith({ programId: 'program-1', query: 'observation', limit: 30 }));
    expect(await mocks.search.mock.results.at(-1)?.value).toEqual(ok([{ sourceId: 'source-1', versionId: 'version-1', title: 'Field notes', excerpt: '…careful observation…' }]));
    expect(await screen.findByText('Matches in saved text')).toBeVisible();
    expect(mocks.search).toHaveBeenCalledWith({ programId: 'program-1', query: 'observation', limit: 30 });
    await user.click(screen.getByRole('button', { name: /Field notes.*careful observation/ }));
    await user.selectOptions(screen.getByLabelText('Freshness policy'), 'fixed');
    await waitFor(() => expect(mocks.policy).toHaveBeenCalledWith(expect.objectContaining({ sourceId: 'source-1', freshnessPolicy: 'fixed', expectedRevision: 4 })));
  });

  it('adopts a pending version explicitly and continues to describe check time accurately', async () => {
    const user = userEvent.setup();
    mocks.adopt.mockResolvedValue(ok({ ...workspace, sources: [{ ...source, activeVersionId: 'version-2', pendingVersionId: null, activeVersion: pendingText, pendingVersion: null, revision: 5 }] }));
    renderSources();
    await user.click(await screen.findByRole('button', { name: 'Review update' }));
    await user.click(screen.getByRole('button', { name: 'Adopt this version' }));
    await waitFor(() => expect(mocks.adopt).toHaveBeenCalledWith(expect.objectContaining({ versionId: 'version-2', expectedRevision: 4 })));
    expect(await screen.findByText(/A changed representation was adopted/)).toBeVisible();
  });

  it('pauses after a CAS conflict and reloads current source state', async () => {
    const user = userEvent.setup();
    mocks.policy.mockResolvedValueOnce(fail('Source changed; reload and retry.')).mockResolvedValueOnce(ok(workspace));
    renderSources();
    await user.selectOptions(await screen.findByLabelText('Freshness policy'), 'fixed');
    expect(await screen.findByText(/Reload the latest version/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Reload sources' }));
    await waitFor(() => expect(mocks.workspace).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('serializes mutations so another source action cannot start while a check is in flight', async () => {
    const user = userEvent.setup();
    let finish!: (value: ReturnType<typeof ok<LearningSourceWorkspaceDto>>) => void;
    mocks.refresh.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    renderSources();
    await user.click(await screen.findByRole('button', { name: 'Check for changes' }));
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Add material' })).toBeDisabled();
    expect(screen.getByLabelText('Freshness policy')).toBeDisabled();
    finish(ok(workspace));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Add material' })).toBeEnabled());
  });

  it('offers a failed-check retry with the same operation after selecting another source', async () => {
    const user = userEvent.setup();
    const secondVersion = { ...activeText, id: 'version-3', title: 'Second archive' };
    const second = { ...source, id: 'source-2', pendingVersionId: null, pendingVersion: null, requestedUrl: 'https://archive.example/second', activeVersionId: 'version-3', activeVersion: secondVersion, versions: [secondVersion] };
    const multi = { programId: 'program-1', sources: [source, second] };
    mocks.refresh.mockResolvedValueOnce(fail('Temporary network error')).mockResolvedValueOnce(ok(multi));
    renderSources(multi);
    await user.click(await screen.findByRole('button', { name: 'Check for changes' }));
    expect(await screen.findByRole('button', { name: 'Retry same operation' })).toBeVisible();
    const first = mocks.refresh.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: /Second archive/ }));
    await user.click(screen.getByRole('button', { name: 'Retry same operation' }));
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(2));
    expect(mocks.refresh.mock.calls[1][0]).toEqual(first);
  });

  it('supports accessible add-mode focus, Escape, and focus restoration; the narrow flow remains stacked', async () => {
    const user = userEvent.setup();
    renderSources();
    const trigger = await screen.findByRole('button', { name: 'Add material' });
    expect(await screen.findByLabelText('Freshness policy')).toHaveClass('w-full', 'min-w-0', 'max-w-full');
    await user.click(trigger);
    expect(screen.getByLabelText('Web address')).toHaveFocus();
    await user.click(screen.getByRole('button', { name: 'Paste text' }));
    await waitFor(() => expect(screen.getByLabelText('Title')).toHaveFocus());
    await user.keyboard('{Escape}');
    await waitFor(() => expect(trigger).toHaveFocus());
    const grid = screen.getByTestId('learning-sources-workspace').querySelector(':scope > .grid');
    expect(grid).toHaveClass('lg:grid-cols-[minmax(230px,0.36fr)_minmax(0,1fr)]');
    expect(grid).not.toHaveClass('grid-cols-2');
  });

  it('keeps loading, empty, and failure states distinct', async () => {
    mocks.workspace.mockReturnValue(new Promise(() => undefined));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const loading = render(<QueryClientProvider client={client}><SourcesPanel programId="program-1" /></QueryClientProvider>);
    expect(screen.getByRole('status')).toHaveTextContent('Opening the source library');
    loading.unmount();
    mocks.workspace.mockResolvedValue(ok({ programId: 'program-1', sources: [] }));
    renderSources({ programId: 'program-1', sources: [] });
    expect(await screen.findByText('No materials have been saved to this program yet.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Add first material' })).toBeVisible();
  });

  it('shows a retryable workspace error instead of an empty library', async () => {
    mocks.workspace.mockRejectedValueOnce(new Error('Could not read source library')).mockResolvedValueOnce(ok({ programId: 'program-1', sources: [] }));
    renderSources();
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not read source library');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByText('No materials have been saved to this program yet.')).toBeVisible();
  });
});
