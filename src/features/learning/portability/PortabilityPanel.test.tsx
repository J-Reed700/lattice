import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CONFLICT_POLICIES } from '@/features/learning/portability/conflictPolicies';
import { PortabilityPanel } from '@/features/learning/portability/PortabilityPanel';

const mocks = vi.hoisted(() => ({ open: vi.fn(), workspace: vi.fn(), exportPack: vi.fn(), saveAs: vi.fn(), preview: vi.fn(), apply: vi.fn(), cancel: vi.fn(), showInFolder: vi.fn(), onImported: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPortabilityWorkspace: mocks.workspace,
  exportLearningPack: mocks.exportPack,
  saveLearningPackAs: mocks.saveAs,
  previewLearningPackImport: mocks.preview,
  applyLearningPackImport: mocks.apply,
  cancelLearningPackImportPreview: mocks.cancel,
  showInFolder: mocks.showInFolder,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const manifest = (overrides: Record<string, unknown> = {}) => ({ format: 'lattice.learning-pack', version: 4, schemaVersion: 20261013000000, packId: 'pack-1', title: 'Evidence basics', createdAt: 1_790_000_000_000, applicationVersion: '1.0.0', rootSha256: 'root-checksum', privacy: { includesPrivateChat: false, includesCredentials: false, includesAnswerKeys: true, includesHiddenEvaluators: false, includesLearnerEvidence: true, includesPracticalArtifacts: true, includesFullSourceBodies: true, sourceBodyRedistributionConfirmed: true, omittedItems: ['Unrelated private chat', 'Hosted runtime secrets'] }, entries: [{ path: 'program.json', kind: 'program', sha256: 'sha', bytes: 42 }], ...overrides });
const preview = (overrides: Record<string, unknown> = {}) => ({ id: 'preview-1', sourcePath: '/tmp/evidence.lattice-learning', manifest: manifest(), incomingProgramId: 'program-1', incomingProgramTitle: 'Evidence basics', conflictPolicy: 'merge_safe', conflicts: [], changes: [{ entityKind: 'program', entityId: 'program-1', action: 'import', description: 'Import Evidence basics' }], warnings: ['A historical source was truncated.'], status: 'pending', canApply: true, createdAt: 1_790_000_000_000, decidedAt: null, ...overrides });
const notice = (overrides: Record<string, unknown> = {}) => ({ activeJob: false, notebookNoteCount: 0, practicalDraftCount: 0, ...overrides });
const workspace = (overrides: Record<string, unknown> = {}) => ({ programId: 'program-1', exports: [], importPreviews: [], imports: [], sourceWorkspace: { programId: 'program-1', sources: [] }, exportNotice: notice(), ...overrides });
const exportRow = (destinationPath: string) => ({ id: 'export-1', programId: 'program-1', destinationPath, manifest: manifest(), createdAt: 1_790_000_000_000 });

function renderPanel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  return render(<PortabilityPanel programId="program-1" programTitle="Evidence basics" onImported={mocks.onImported} />, { wrapper });
}

describe('Learning Studio portable packs', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.open.mockResolvedValue('/tmp/evidence.lattice-learning');
    mocks.workspace.mockResolvedValue(ok(workspace()));
    mocks.exportPack.mockResolvedValue(ok(workspace({ exports: [exportRow('/managed/learning/evidence.lattice-learning')] })));
    mocks.saveAs.mockResolvedValue(ok({ cancelled: false, export: exportRow('/Volumes/USB/evidence-basics.lattice-learning') }));
    mocks.preview.mockResolvedValue(ok(workspace({ importPreviews: [preview()] })));
    mocks.apply.mockResolvedValue(ok(workspace({ importPreviews: [preview({ status: 'applied' })], imports: [{ id: 'import-1', previewId: 'preview-1', importedProgramId: 'program-1', backupId: null, appliedChanges: [{ entityKind: 'program', entityId: 'program-1', action: 'created', description: 'Imported program.' }], importedAt: 1_790_000_000_000 }] })));
    mocks.cancel.mockResolvedValue(ok(workspace({ importPreviews: [preview({ status: 'cancelled' })] })));
    mocks.showInFolder.mockResolvedValue(ok(null));
  });

  it('presents the move heading and defaults evidence and practical work on, full source text off', async () => {
    renderPanel();
    expect(await screen.findByRole('heading', { name: 'Move this course to another computer' })).toBeVisible();
    expect(screen.getByLabelText(/Include learner evidence/)).toBeChecked();
    expect(screen.getByLabelText(/Include practical work artifacts/)).toBeChecked();
    expect(screen.getByLabelText(/Include full source text/)).not.toBeChecked();
    expect(screen.getByText(/the other computer gets excerpts only; web references can be refreshed there/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save to…' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Export to app folder' })).toBeEnabled();
  });

  it('requires explicit redistribution consent and retries an export with the exact request after a lost response', async () => {
    const user = userEvent.setup();
    mocks.exportPack.mockResolvedValueOnce(fail('Connection interrupted'));
    renderPanel();
    await screen.findByRole('button', { name: 'Export to app folder' });
    await user.click(screen.getByLabelText(/Include full source text/));
    const exportButton = screen.getByRole('button', { name: 'Export to app folder' });
    expect(exportButton).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save to…' })).toBeDisabled();
    await user.click(screen.getByLabelText(/I confirm that full source bodies/));
    await user.click(exportButton);
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    const request = mocks.exportPack.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'program-1', fileName: 'evidence-basics', includeEvidence: true, includePracticalArtifacts: true, includeSourceBodies: true, sourceBodyRedistributionConfirmed: true });
    await user.click(screen.getByRole('button', { name: 'Retry same export' }));
    await waitFor(() => expect(mocks.exportPack).toHaveBeenCalledTimes(2));
    expect(mocks.exportPack.mock.calls[1][0]).toEqual(request);
    expect(await screen.findByRole('status')).toHaveTextContent(/^Course file saved to \/managed\/learning/);
    await user.click(screen.getByRole('button', { name: 'Reveal file' }));
    expect(mocks.showInFolder).toHaveBeenCalledWith('/managed/learning/evidence.lattice-learning');
  });

  it('saves to a file the user picks, retries a failed save with the same request, and treats a closed dialog as a no-op', async () => {
    const user = userEvent.setup();
    mocks.saveAs.mockResolvedValueOnce(ok({ cancelled: true, export: null })).mockResolvedValueOnce(fail('The destination is read-only'));
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Save to…' }));
    await waitFor(() => expect(mocks.saveAs).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    const first = mocks.saveAs.mock.calls[0][0];
    expect(first).toMatchObject({ programId: 'program-1', fileName: 'evidence-basics', includeEvidence: true, includePracticalArtifacts: true, includeSourceBodies: false, sourceBodyRedistributionConfirmed: false, operationId: expect.any(String) });

    await user.click(screen.getByRole('button', { name: 'Save to…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The destination is read-only');
    const failed = mocks.saveAs.mock.calls[1][0];
    expect(failed.operationId).not.toBe(first.operationId);
    await user.click(screen.getByRole('button', { name: 'Retry same save' }));
    await waitFor(() => expect(mocks.saveAs).toHaveBeenCalledTimes(3));
    expect(mocks.saveAs.mock.calls[2][0]).toEqual(failed);
    expect(await screen.findByRole('status')).toHaveTextContent('Course file saved to /Volumes/USB/evidence-basics.lattice-learning');
    await user.click(screen.getByRole('button', { name: 'Reveal file' }));
    expect(mocks.showInFolder).toHaveBeenCalledWith('/Volumes/USB/evidence-basics.lattice-learning');
    expect(mocks.exportPack).not.toHaveBeenCalled();
  });

  it('blocks both exports while the course is still being prepared and names what stays on this computer', async () => {
    mocks.workspace.mockResolvedValue(ok(workspace({ exportNotice: notice({ activeJob: true, notebookNoteCount: 2, practicalDraftCount: 1 }) })));
    renderPanel();
    expect(await screen.findByText(/This course is still being prepared/)).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save to…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Export to app folder' })).toBeDisabled();
    expect(screen.getByText(/2 lesson notes and 1 lab draft stay on this computer\./)).toBeVisible();
  });

  it('checks again while the course is being prepared and lifts the lock when it is ready', async () => {
    vi.useFakeTimers();
    try {
      mocks.workspace.mockResolvedValueOnce(ok(workspace({ exportNotice: notice({ activeJob: true }) })));
      renderPanel();
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(screen.getByText(/This course is still being prepared/)).toBeVisible();
      expect(screen.getByRole('button', { name: 'Save to…' })).toBeDisabled();
      // The poll fires at 5 s; the response then renders on the next timer turns.
      await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
      await act(async () => { await vi.advanceTimersByTimeAsync(100); });
      expect(mocks.workspace).toHaveBeenCalledTimes(2);
      expect(screen.queryByText(/This course is still being prepared/)).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save to…' })).toBeEnabled();
      // Nothing is being prepared any more, so it stops asking.
      await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
      expect(mocks.workspace).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('uses the singular when one lesson note stays behind and says nothing when nothing does', async () => {
    mocks.workspace.mockResolvedValueOnce(ok(workspace({ exportNotice: notice({ notebookNoteCount: 1 }) })));
    const { unmount } = renderPanel();
    expect(await screen.findByText(/^1 lesson note stays on this computer\./)).toBeVisible();
    unmount();
    renderPanel();
    await screen.findByRole('button', { name: 'Save to…' });
    expect(screen.queryByText(/stays? on this computer/)).not.toBeInTheDocument();
  });

  it('imports through the move flow, previewing as a move first, and reports the imported course', async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Choose a course file…' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(1));
    expect(mocks.preview.mock.calls[0][0]).toMatchObject({ sourcePath: '/tmp/evidence.lattice-learning', conflictPolicy: 'merge_safe' });
    expect(await screen.findByText('Dry run · pending')).toBeVisible();
    expect(screen.getByRole('heading', { level: 4, name: 'Bring a course to this computer' })).toBeVisible();
    expect(screen.getByRole('heading', { level: 5, name: 'Evidence basics' })).toBeVisible();
    expect(screen.getByText(CONFLICT_POLICIES[0].label)).toBeVisible();
    expect(screen.getByText(CONFLICT_POLICIES[0].description)).toBeVisible();
    expect(screen.getByRole('region', { name: 'Pack privacy manifest' })).toHaveTextContent('Hosted runtime secrets');
    expect(screen.getByText(/A historical source was truncated/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Apply these changes' }));
    await user.click(await screen.findByRole('button', { name: 'Confirm import' }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledTimes(1));
    expect(await screen.findByText('“Evidence basics” is ready on this computer.')).toBeVisible();
    expect(mocks.onImported).toHaveBeenCalledWith('program-1');
  });

  it('shows no Apply for a preview the backend marks ineligible', async () => {
    const user = userEvent.setup();
    mocks.preview.mockResolvedValue(ok(workspace({ importPreviews: [preview({ canApply: false, warnings: ['Pack digest failed verification.'] })] })));
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Choose a course file…' }));
    expect(await screen.findByText(/Pack digest failed verification/)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Apply these changes' })).not.toBeInTheDocument();
    expect(screen.getByText(/This preview is not eligible to apply/)).toBeVisible();
    expect(mocks.apply).not.toHaveBeenCalled();
  });

  it('surfaces an unavailable history and retries it', async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValueOnce(fail('History table is locked'));
    renderPanel();
    expect(await screen.findByRole('alert')).toHaveTextContent('History table is locked');
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByRole('heading', { name: 'Move this course to another computer' })).toBeVisible();
  });

  it('lists earlier exports and imports and reveals an exported file', async () => {
    const user = userEvent.setup();
    mocks.workspace.mockResolvedValue(ok(workspace({ exports: [exportRow('/managed/learning/evidence.lattice-learning')], imports: [{ id: 'import-1', previewId: 'preview-1', importedProgramId: 'program-1', backupId: 'backup-1', appliedChanges: [], importedAt: 1_790_000_000_000 }] })));
    renderPanel();
    expect(await screen.findByText('1 exports · 1 imports')).toBeVisible();
    expect(screen.getByText(/backup backup-1/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Reveal Evidence basics in folder' }));
    expect(mocks.showInFolder).toHaveBeenCalledWith('/managed/learning/evidence.lattice-learning');
  });
});
