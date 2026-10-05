import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PortabilityPanel } from '@/features/learning/portability/PortabilityPanel';

const mocks = vi.hoisted(() => ({ open: vi.fn(), workspace: vi.fn(), exportPack: vi.fn(), preview: vi.fn(), apply: vi.fn(), cancel: vi.fn(), showInFolder: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPortabilityWorkspace: mocks.workspace,
  exportLearningPack: mocks.exportPack,
  previewLearningPackImport: mocks.preview,
  applyLearningPackImport: mocks.apply,
  cancelLearningPackImportPreview: mocks.cancel,
  showInFolder: mocks.showInFolder,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const manifest = (overrides: Record<string, unknown> = {}) => ({ format: 'lattice.learning-pack', version: 1, packId: 'pack-1', title: 'Evidence basics', createdAt: 1_790_000_000_000, applicationVersion: '1.0.0', rootSha256: 'root-checksum', privacy: { includesPrivateChat: false, includesCredentials: false, includesAnswerKeys: true, includesHiddenEvaluators: false, includesLearnerEvidence: true, includesPracticalArtifacts: true, includesFullSourceBodies: true, sourceBodyRedistributionConfirmed: true, omittedItems: ['Unrelated private chat', 'Hosted runtime secrets'] }, entries: [{ path: 'program.json', kind: 'program', sha256: 'sha', bytes: 42 }], ...overrides });
const preview = (overrides: Record<string, unknown> = {}) => ({ id: 'preview-1', sourcePath: '/tmp/evidence.lattice-learning', manifest: manifest(), incomingProgramId: 'incoming-1', incomingProgramTitle: 'Evidence basics', conflictPolicy: 'create_copy', conflicts: [{ entityKind: 'source', incomingId: 'source-1', incomingTitle: 'Field guide', existingId: 'local-source', existingTitle: 'Field guide', resolution: 'create_copy' }], changes: [{ entityKind: 'program', entityId: 'incoming-1', action: 'create', description: 'Create a separate copy with its own program identity.' }], warnings: ['A historical source was truncated.'], status: 'pending', canApply: true, createdAt: 1_790_000_000_000, decidedAt: null, ...overrides });
const workspace = (overrides: Record<string, unknown> = {}) => ({ programId: 'program-1', exports: [], importPreviews: [], imports: [], sourceWorkspace: { programId: 'program-1', sources: [] }, ...overrides });

function renderPanel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  return render(<PortabilityPanel programId="program-1" programTitle="Evidence basics" />, { wrapper });
}

describe('Learning Studio portable packs', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.open.mockResolvedValue('/tmp/evidence.lattice-learning');
    mocks.workspace.mockResolvedValue(ok(workspace()));
    mocks.exportPack.mockResolvedValue(ok(workspace({ exports: [{ id: 'export-1', programId: 'program-1', destinationPath: '/managed/learning/evidence.lattice-learning', manifest: manifest(), createdAt: 1_790_000_000_000 }] })));
    mocks.preview.mockResolvedValue(ok(workspace({ importPreviews: [preview()] })));
    mocks.apply.mockResolvedValue(ok(workspace({ imports: [{ id: 'import-1', previewId: 'preview-1', importedProgramId: 'imported-1', backupId: null, appliedChanges: [{ entityKind: 'program', entityId: 'imported-1', action: 'created', description: 'Copied program.' }], importedAt: 1_790_000_000_000 }] })));
    mocks.cancel.mockResolvedValue(ok(workspace({ importPreviews: [preview({ status: 'cancelled' })] })));
    mocks.showInFolder.mockResolvedValue(ok(null));
  });

  it('requires explicit redistribution consent and retries an export with the exact request after a lost response', async () => {
    const user = userEvent.setup();
    mocks.exportPack.mockResolvedValueOnce(fail('Connection interrupted'));
    renderPanel();
    await screen.findByRole('button', { name: 'Export program pack' });
    const includeBodies = screen.getByLabelText(/Include full source text/);
    await user.click(includeBodies);
    const exportButton = screen.getByRole('button', { name: 'Export program pack' });
    expect(exportButton).toBeDisabled();
    await user.click(screen.getByLabelText(/I confirm that full source bodies/));
    await user.click(exportButton);
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    const request = mocks.exportPack.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'program-1', includeSourceBodies: true, sourceBodyRedistributionConfirmed: true });
    await user.click(screen.getByRole('button', { name: 'Retry same export' }));
    await waitFor(() => expect(mocks.exportPack).toHaveBeenCalledTimes(2));
    expect(mocks.exportPack.mock.calls[1][0]).toEqual(request);
    expect(await screen.findByRole('status')).toHaveTextContent(/Pack saved to \/managed\/learning/);
    await user.click(screen.getByRole('button', { name: 'Reveal file' }));
    expect(mocks.showInFolder).toHaveBeenCalledWith('/managed/learning/evidence.lattice-learning');
  });

  it('previews all conflicts and warnings before an explicit import apply', async () => {
    const user = userEvent.setup();
    renderPanel();
    await screen.findByRole('button', { name: 'Choose pack and preview' });
    await user.click(screen.getByRole('button', { name: 'Choose pack and preview' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(1));
    expect(await screen.findByText('Evidence basics')).toBeVisible();
    expect(screen.getByText('Dry run · pending')).toBeVisible();
    const privacy = screen.getByRole('region', { name: 'Pack privacy manifest' });
    expect(privacy).toHaveTextContent('Answer keys included');
    expect(privacy).toHaveTextContent('Hidden evaluators included');
    expect(privacy).toHaveTextContent('Learner evidence included');
    expect(privacy).toHaveTextContent('Practical artifacts included');
    expect(privacy).toHaveTextContent('Full source bodies included');
    expect(privacy).toHaveTextContent('Source redistribution confirmed');
    expect(privacy).toHaveTextContent('Omitted items');
    expect(privacy).toHaveTextContent('Hosted runtime secrets');
    expect(screen.getByText(/Restorable assessments include their model-authored answer keys/)).toBeVisible();
    expect(screen.getByText(/A historical source was truncated/)).toBeVisible();
    expect(screen.getByText(/Create a separate copy with its own program identity/)).toBeVisible();
    expect(mocks.preview.mock.calls[0][0]).toMatchObject({ sourcePath: '/tmp/evidence.lattice-learning', conflictPolicy: 'create_copy' });
    expect(mocks.apply).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Apply these changes' }));
    expect(await screen.findByRole('dialog', { name: 'Apply the reviewed import?' })).toBeVisible();
    expect(mocks.apply).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Confirm import' }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledTimes(1));
    expect(mocks.apply.mock.calls[0][0]).toEqual({ previewId: 'preview-1', expectedRootSha256: 'root-checksum', operationId: expect.any(String) });
    expect(await screen.findByText('Imported program imported-1.')).toBeVisible();
  });

  it('keeps the same import preview identity on retry, and permits cancelling a pending preview', async () => {
    const user = userEvent.setup();
    mocks.preview.mockResolvedValueOnce(fail('Pack read was interrupted'));
    renderPanel();
    await screen.findByRole('button', { name: 'Choose pack and preview' });
    await user.click(screen.getByRole('button', { name: 'Choose pack and preview' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Pack read was interrupted');
    const original = mocks.preview.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry same preview' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(2));
    expect(mocks.preview.mock.calls[1][0]).toEqual(original);
    await user.click(await screen.findByRole('button', { name: 'Cancel preview' }));
    expect(mocks.cancel).toHaveBeenCalledWith({ previewId: 'preview-1', operationId: expect.any(String) });
  });

  it('does not allow applying a preview that backend marks ineligible', async () => {
    const user = userEvent.setup();
    mocks.preview.mockResolvedValue(ok(workspace({ importPreviews: [preview({ canApply: false, warnings: ['Pack digest failed verification.'] })] })));
    renderPanel();
    await screen.findByRole('button', { name: 'Choose pack and preview' });
    await user.click(screen.getByRole('button', { name: 'Choose pack and preview' }));
    expect(await screen.findByText(/Pack digest failed verification/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Apply these changes' })).toBeDisabled();
    expect(mocks.apply).not.toHaveBeenCalled();
  });
});
