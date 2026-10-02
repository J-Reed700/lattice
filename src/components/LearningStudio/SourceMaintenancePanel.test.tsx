import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SourceMaintenancePanel } from './SourceMaintenancePanel';

const mocks = vi.hoisted(() => ({ workspace: vi.fn(), version: vi.fn(), search: vi.fn(), selector: vi.fn(), remove: vi.fn(), reimport: vi.fn(), semantic: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningSourceWorkspace: mocks.workspace,
  getLearningSourceVersion: mocks.version,
  searchLearningSources: mocks.search,
  deleteLearningSource: mocks.remove,
  reimportLearningSource: mocks.reimport,
  createLearningSourceSelector: mocks.selector,
  searchLearningSourcesSemantically: mocks.semantic,
} }));
const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const source = (deletedAt: number | null = null) => ({ id: 'source-1', kind: 'text', origin: 'Field manual', requestedUrl: null, freshnessPolicy: 'fixed', activeVersionId: deletedAt ? null : 'version-2', pendingVersionId: null, revision: 4, activeVersion: deletedAt ? null : { id: 'version-2', versionNumber: 2, title: 'Field manual', publisher: null, resolvedUrl: null, excerpt: 'A method for checking evidence.', contentSha256: 'sha-two', wordCount: 12, truncated: false, extractionVersion: 'text-v1', acquiredAt: 1_790_000_000_000 }, pendingVersion: null, versions: [{ id: 'version-2', versionNumber: 2, title: 'Field manual', publisher: null, resolvedUrl: null, excerpt: 'A method for checking evidence.', contentSha256: 'sha-two', wordCount: 12, truncated: false, extractionVersion: 'text-v1', acquiredAt: 1_790_000_000_000 }, { id: 'version-1', versionNumber: 1, title: 'Field manual · original', publisher: null, resolvedUrl: null, excerpt: 'Earlier version.', contentSha256: 'sha-one', wordCount: 8, truncated: false, extractionVersion: 'text-v1', acquiredAt: 1_780_000_000_000 }], latestCheck: null, checks: [], createdAt: 1_780_000_000_000, updatedAt: 1_790_000_000_000, deletedAt, deletionReason: deletedAt ? 'Superseded source' : null });
const workspace = (item = source()) => ({ programId: 'program-1', sources: [item] });
const fullText = 'Before 🦀 ownership transfers after acknowledgement. After.';
function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<SourceMaintenancePanel programId="program-1" />, { wrapper });
}

describe('Learning Studio source continuity', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.workspace.mockResolvedValue(ok(workspace()));
    mocks.version.mockResolvedValue(ok({ sourceId: 'source-1', version: source().versions[0], fullText, usage: [{ lessonId: 'lesson-1', lessonTitle: 'Inspect evidence', referenceKind: 'lesson', referenceTitle: 'Source note' }] }));
    mocks.search.mockResolvedValue(ok([]));
    mocks.selector.mockResolvedValue(ok({ id: 'selector-1', sourceId: 'source-1', sourceVersionId: 'version-2', selector: { exact: '🦀 ownership transfers', prefix: 'Before ', suffix: ' after acknowledgement.' }, matchStatus: 'exact', startByte: 7, endByte: 32, candidateCount: 1, createdAt: 1_790_000_000_000 }));
    mocks.remove.mockResolvedValue(ok(workspace(source(1_790_100_000_000))));
    mocks.reimport.mockResolvedValue(ok(workspace()));
    mocks.semantic.mockResolvedValue(ok([{ sourceId: 'source-1', version: source().versions[1], excerpt: 'Earlier saved quote.', score: 0.77, retrievalKind: 'semantic', selector: { exact: 'Earlier saved quote.', prefix: '', suffix: '' } }]));
  });

  it('saves exact quote selectors with UTF-8 byte offsets from the selected historical text', async () => {
    const user = userEvent.setup();
    renderPanel();
    const reader = await screen.findByRole('textbox', { name: 'Saved source text for Field manual' }) as HTMLTextAreaElement;
    const start = fullText.indexOf('🦀');
    const end = start + '🦀 ownership transfers'.length;
    reader.setSelectionRange(start, end);
    fireEvent.select(reader);
    await user.click(screen.getByRole('button', { name: 'Save exact quote' }));
    await waitFor(() => expect(mocks.selector).toHaveBeenCalledTimes(1));
    const req = mocks.selector.mock.calls[0][0];
    expect(req).toMatchObject({ programId: 'program-1', sourceId: 'source-1', sourceVersionId: 'version-2' });
    expect(req.startByte).toBe(new TextEncoder().encode(fullText.slice(0, start)).length);
    expect(req.endByte).toBe(new TextEncoder().encode(fullText.slice(0, end)).length);
    expect(await screen.findByRole('status')).toHaveTextContent('Quote saved · exact match');
  });

  it('requires a removal reason and retries the identical tombstone action', async () => {
    const user = userEvent.setup();
    mocks.remove.mockResolvedValueOnce(fail('Connection interrupted'));
    renderPanel();
    await screen.findByRole('textbox', { name: 'Saved source text for Field manual' });
    await user.click(screen.getByRole('button', { name: 'Remove source' }));
    const confirm = screen.getByRole('button', { name: 'Confirm removal' });
    expect(confirm).toBeDisabled();
    await user.type(screen.getByLabelText('Reason for removal'), 'Superseded source');
    await user.click(confirm);
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    const request = mocks.remove.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'program-1', sourceId: 'source-1', expectedRevision: 4, reason: 'Superseded source' });
    // The retry belongs to the failed delete, even if its confirmation is dismissed.
    await user.click(screen.getByRole('button', { name: 'Keep source' }));
    await user.click(screen.getByRole('button', { name: 'Retry same removal' }));
    await waitFor(() => expect(mocks.remove).toHaveBeenCalledTimes(2));
    expect(mocks.remove.mock.calls[1][0]).toEqual(request);
  });

  it('shows a tombstone and lets the learner restore a selected historical version', async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValue(ok(workspace(source(1_790_100_000_000))));
    renderPanel();
    expect(await screen.findByText('Historical tombstone')).toBeVisible();
    expect(screen.getByText(/Superseded source/)).toBeVisible();
    await user.selectOptions(screen.getByLabelText('Saved version'), 'version-1');
    await user.click(screen.getByRole('button', { name: 'Re-import selected version' }));
    await waitFor(() => expect(mocks.reimport).toHaveBeenCalledTimes(1));
    expect(mocks.reimport.mock.calls[0][0]).toMatchObject({ sourceId: 'source-1', versionId: 'version-1', expectedRevision: 4, replacementText: null, operationId: expect.any(String) });
    expect(await screen.findByRole('status')).toHaveTextContent(/Version 1 was restored/);
  });

  it('searches locally stored program versions and opens the exact matching version', async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.type(await screen.findByRole('textbox', { name: 'Search related saved material' }), 'evidence chain');
    await user.click(screen.getByRole('button', { name: 'Search program source versions' }));
    expect(await screen.findByText('Earlier saved quote.')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Open this exact version' }));
    await waitFor(() => expect(mocks.version).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-1', versionId: 'version-1' }));
    expect(screen.getByText(/identifies whether retrieval was semantic or lexical/)).toBeVisible();
  });
});
