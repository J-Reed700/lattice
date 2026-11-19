import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { QuizSourceLine } from '@/features/learning/quizzes/QuizSourceLine';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { LearningSourceDto } from '@/lib/bindings';

const sources: LearningSourceDto[] = [
  { id: 'version-1', title: 'Field notes', url: null, excerpt: '', acquiredAt: 0 },
  { id: 'version-2', title: 'Lab manual', url: null, excerpt: '', acquiredAt: 0 },
];
const versionSummary = (id: string, title: string) => ({
  id, versionNumber: 1, title, publisher: null, resolvedUrl: null, excerpt: '', contentSha256: `sha-${id}`, wordCount: 5,
  truncated: false, extractionVersion: 'reader-1', acquiredAt: 0, snapshotHash: null,
});
const libraryItem = (id: string, version: ReturnType<typeof versionSummary>) => ({
  id, kind: 'web', addedBy: 'user', origin: 'web', requestedUrl: null, freshnessPolicy: 'manual', activeVersionId: version.id,
  pendingVersionId: null, revision: 1, deletedAt: null, deletionReason: null, activeVersion: version, pendingVersion: null,
  versions: [version], latestCheck: null, checks: [], createdAt: 0, updatedAt: 0,
});
const v1 = versionSummary('version-1', 'Field notes');
const v2 = versionSummary('version-2', 'Lab manual');
const workspace = { programId: 'program-1', recheck: null, sources: [libraryItem('source-1', v1), libraryItem('source-2', v2)] };
const ok = <T,>(data: T) => ({ ok: true as const, data });

const getWorkspace = vi.fn();
const getVersion = vi.fn();

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  const api = { getLearningSourceWorkspace: getWorkspace, getLearningSourceVersion: getVersion } as unknown as ApiClient;
  return (
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    </ApiProvider>
  );
}

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe() {}
    unobserve() {}
    disconnect() {}
  });
  getWorkspace.mockReset().mockResolvedValue(ok(workspace));
  getVersion.mockReset().mockImplementation(async (request: { sourceId: string; versionId: string }) => ok({
    sourceId: request.sourceId,
    version: request.versionId === 'version-1' ? v1 : v2,
    fullText: 'Full text of the saved source.',
    usage: [],
  }));
  useCitationDisplayStore.setState({ visible: true });
  useReaderStore.setState({ session: null });
});

afterEach(() => {
  vi.unstubAllGlobals();
  useCitationDisplayStore.setState({ visible: true });
  useReaderStore.setState({ session: null });
});

describe('QuizSourceLine with openable sources', () => {
  it('shows a labelled chips row with a chip and title for each distinct cited source', () => {
    render(<QuizSourceLine ids={['version-1', 'version-2', 'version-1']} sources={sources} label="Based on" programId="program-1" ownerKey="answer-1" />, { wrapper });

    expect(screen.getByText('Based on')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /^Citation \d/ })).toHaveLength(2);
    expect(screen.getByRole('button', { name: /Citation 1/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Citation 2/ })).toBeInTheDocument();
    expect(screen.getAllByText('Field notes')).toHaveLength(1);
    expect(screen.getByText('Lab manual')).toBeInTheDocument();
  });

  it('skips ids that are not among the sources', () => {
    render(<QuizSourceLine ids={['missing', 'version-2']} sources={sources} programId="program-1" ownerKey="answer-1" />, { wrapper });

    expect(screen.getAllByRole('button', { name: /^Citation \d/ })).toHaveLength(1);
    expect(screen.getByText('Lab manual')).toBeInTheDocument();
  });

  it('opens the docked reader for the owner on that version', async () => {
    render(<QuizSourceLine ids={['version-1', 'version-2']} sources={sources} programId="program-1" ownerKey="answer-1" />, { wrapper });

    fireEvent.click(screen.getByRole('button', { name: /Citation 2/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));

    await waitFor(() => expect(useReaderStore.getState().session).not.toBeNull());
    const session = useReaderStore.getState().session!;
    expect(session.ownerKey).toBe('answer-1');
    expect(session.index).toBe(1);
    expect(session.citations).toHaveLength(2);
    expect(session.citations[1]!.fileName).toBe('Lab manual');
    expect(getVersion).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-2', versionId: 'version-2' });
  });

  it('renders nothing in reading mode', () => {
    useCitationDisplayStore.setState({ visible: false });
    const { container } = render(<QuizSourceLine ids={['version-1']} sources={sources} programId="program-1" ownerKey="answer-1" />, { wrapper });

    expect(container).toBeEmptyDOMElement();
  });
});

describe('QuizSourceLine as a plain line', () => {
  it('names the distinct source titles without chips when not openable', () => {
    render(<QuizSourceLine ids={['version-1', 'version-2', 'version-1']} sources={sources} />, { wrapper });

    expect(screen.getByText('Based on: Field notes · Lab manual')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('stays plain with a programId but no ownerKey, and honours the label', () => {
    render(<QuizSourceLine ids={['version-1']} sources={sources} programId="program-1" label="From" />, { wrapper });

    expect(screen.getByText('From: Field notes')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('falls back to "the lesson" when no cited id is a known source', () => {
    render(<QuizSourceLine ids={['missing']} sources={sources} />, { wrapper });

    expect(screen.getByText('Based on: the lesson')).toBeInTheDocument();
  });

  it('renders nothing without ids', () => {
    const { container } = render(<QuizSourceLine ids={[]} sources={sources} programId="program-1" ownerKey="answer-1" />, { wrapper });

    expect(container).toBeEmptyDOMElement();
  });

  it('hides the plain line in reading mode', () => {
    useCitationDisplayStore.setState({ visible: false });
    const { container } = render(<QuizSourceLine ids={['version-1']} sources={sources} />, { wrapper });

    expect(within(container).queryByText(/Based on/)).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });
});
