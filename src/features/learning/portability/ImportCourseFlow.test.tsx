import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CONFLICT_POLICIES } from '@/features/learning/portability/conflictPolicies';
import { ImportCourseFlow } from '@/features/learning/portability/ImportCourseFlow';
import { deferred } from '@/tests/deferred';

const mocks = vi.hoisted(() => ({ open: vi.fn(), preview: vi.fn(), apply: vi.fn(), cancel: vi.fn(), flush: vi.fn(), onImported: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));
vi.mock('@/lib/pendingSaves', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/lib/pendingSaves')>(),
  flushPendingSaves: mocks.flush,
}));
vi.mock('@/lib/api', () => ({ default: {
  previewLearningPackImport: mocks.preview,
  applyLearningPackImport: mocks.apply,
  cancelLearningPackImportPreview: mocks.cancel,
} }));

const label = (value: string) => CONFLICT_POLICIES.find((option) => option.value === value)!.label;
const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const manifest = { format: 'lattice.learning-pack', version: 4, schemaVersion: 20261013000000, packId: 'pack-1', title: 'Field biology', createdAt: 1_790_000_000_000, applicationVersion: '1.0.0', rootSha256: 'root-checksum', privacy: { includesPrivateChat: false, includesCredentials: false, includesAnswerKeys: true, includesHiddenEvaluators: false, includesLearnerEvidence: true, includesPracticalArtifacts: true, includesFullSourceBodies: false, sourceBodyRedistributionConfirmed: false, omittedItems: [] }, entries: [] };
type Conflict = { kind: string; existingId: string | null; existingTitle: string | null; reason: string | null; clearsWith: string[] };
// The choices the backend says clear each kind; every other kind clears only with a copy.
const CLEARS_WITH: Record<string, string[]> = { program: ['replace_after_backup', 'create_copy'], pack_schema: [] };
const conflict = (kind: string, overrides: Partial<Conflict> = {}): Conflict => ({ kind, existingId: 'course-1', existingTitle: 'Field biology (this computer)', reason: null, clearsWith: CLEARS_WITH[kind] ?? ['create_copy'], ...overrides });
const previewOf = (request: { previewId: string; sourcePath: string; conflictPolicy: string }, overrides: Record<string, unknown> = {}) => ({
  id: request.previewId, sourcePath: request.sourcePath, manifest, incomingProgramId: 'course-1', incomingProgramTitle: 'Field biology',
  conflictPolicy: request.conflictPolicy, conflicts: [] as Conflict[], changes: [{ entityKind: 'program', entityId: 'course-1', action: 'import', description: 'Import Field biology' }],
  warnings: [], status: 'pending', canApply: true, createdAt: 1_790_000_000_000, decidedAt: null, ...overrides,
});
const workspace = (programId: string, importPreviews: unknown[], imports: unknown[] = []) => ({ programId, exports: [], importPreviews, imports, sourceWorkspace: { programId, sources: [] }, exportNotice: { staysBehind: [], blockers: [] } });

/** Preview responses per policy; each echoes the request's preview id like the backend does. */
function respondByPolicy(byPolicy: Record<string, Record<string, unknown>>) {
  mocks.preview.mockImplementation(async (request: { previewId: string; sourcePath: string; conflictPolicy: string }) => {
    const overrides = byPolicy[request.conflictPolicy] ?? {};
    return ok(workspace('course-1', [previewOf(request, overrides)]));
  });
}

function renderFlow() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, invalidate, ...render(<ImportCourseFlow headingLevel={2} onImported={mocks.onImported} />, { wrapper }) };
}

describe('Importing a course onto this computer', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.open.mockResolvedValue('/Volumes/USB/field-biology.lattice-learning');
    mocks.flush.mockResolvedValue(true);
    respondByPolicy({});
    mocks.apply.mockImplementation(async (request: { previewId: string }) => ok(workspace('course-1', [previewOf({ previewId: request.previewId, sourcePath: '/Volumes/USB/field-biology.lattice-learning', conflictPolicy: 'merge_safe' }, { status: 'applied' })], [{ id: 'import-1', previewId: request.previewId, importedProgramId: 'course-1', backupId: null, appliedChanges: [], importedAt: 1_790_000_000_000 }])));
    mocks.cancel.mockImplementation(async (request: { previewId: string }) => ok(workspace('course-1', [previewOf({ previewId: request.previewId, sourcePath: '/Volumes/USB/field-biology.lattice-learning', conflictPolicy: 'merge_safe' }, { status: 'cancelled' })])));
  });

  it('moves a course onto a fresh computer with merge_safe and lands on it', async () => {
    const user = userEvent.setup();
    const { invalidate } = renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({ multiple: false, directory: false, filters: [{ name: 'Lattice course', extensions: ['lattice-learning'] }] }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(1));
    const request = mocks.preview.mock.calls[0][0];
    expect(request).toEqual({ sourcePath: '/Volumes/USB/field-biology.lattice-learning', conflictPolicy: 'merge_safe', operationId: expect.any(String), previewId: expect.any(String) });
    expect(screen.getByRole('heading', { level: 2, name: 'Bring a course to this computer' })).toBeVisible();
    expect(await screen.findByRole('heading', { level: 3, name: 'Field biology' })).toBeVisible();
    expect(screen.getByRole('heading', { level: 4, name: 'Pack privacy manifest' })).toBeVisible();
    expect(screen.getByText(label('merge_safe'))).toBeVisible();
    expect(screen.getByText('No ID or content conflicts were found during preview.')).toBeVisible();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Apply these changes' }));
    const confirm = await screen.findByRole('dialog', { name: 'Apply the reviewed import?' });
    expect(confirm).toHaveTextContent(`as “${label('merge_safe')}”`);
    expect(mocks.apply).not.toHaveBeenCalled();
    await user.click(within(confirm).getByRole('button', { name: 'Confirm import' }));
    await waitFor(() => expect(mocks.onImported).toHaveBeenCalledWith('course-1', 'Field biology'));
    expect(mocks.apply.mock.calls[0][0]).toEqual({ previewId: request.previewId, expectedRootSha256: 'root-checksum', operationId: expect.any(String) });
    expect(mocks.flush).toHaveBeenCalled();
    expect(invalidate).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ['learning-programs'] }));
    expect(invalidate).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ['learning-program', 'course-1'] }));
    expect(screen.getByText('Dry run · applied')).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('“Field biology” is ready on this computer.');
  });

  it('offers Replace (default) and a separate copy when the course is already here, and re-previews with Replace', async () => {
    const user = userEvent.setup();
    respondByPolicy({ merge_safe: { conflicts: [conflict('program')], canApply: false } });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    const branch = await screen.findByRole('group', { name: 'This course is already on this computer (Field biology (this computer))' });
    expect(screen.queryByRole('button', { name: 'Apply these changes' })).not.toBeInTheDocument();
    const choices = within(branch).getAllByRole('button');
    expect(choices.map((button) => button.getAttribute('aria-label'))).toEqual(['Replace the copy on this computer', 'Add as a separate copy']);
    expect(choices[0]).toHaveAccessibleDescription(CONFLICT_POLICIES.find((option) => option.value === 'replace_after_backup')!.description);
    const blocked = mocks.preview.mock.calls[0][0];

    await user.click(within(branch).getByRole('button', { name: label('replace_after_backup') }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(2));
    expect(mocks.cancel).toHaveBeenCalledWith({ previewId: blocked.previewId, operationId: expect.any(String) });
    const replace = mocks.preview.mock.calls[1][0];
    expect(replace).toMatchObject({ sourcePath: blocked.sourcePath, conflictPolicy: 'replace_after_backup' });
    expect(replace.previewId).not.toBe(blocked.previewId);
    expect(replace.operationId).not.toBe(blocked.operationId);
    expect(await screen.findByText(label('replace_after_backup'))).toBeVisible();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Apply these changes' }));
    await user.click(await screen.findByRole('button', { name: 'Confirm import' }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledWith(expect.objectContaining({ previewId: replace.previewId })));
    expect(mocks.onImported).toHaveBeenCalledWith('course-1', 'Field biology');
  });

  it('adds a separate copy from the already-here branch', async () => {
    const user = userEvent.setup();
    respondByPolicy({ merge_safe: { conflicts: [conflict('program')], canApply: false } });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: label('create_copy') }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(2));
    expect(mocks.preview.mock.calls[1][0]).toMatchObject({ conflictPolicy: 'create_copy' });
    expect(await screen.findByText(label('create_copy'))).toBeVisible();
  });

  it('offers only a copy when another course owns some of the incoming records', async () => {
    const user = userEvent.setup();
    // For a lesson the backend sends the lesson's local title, not a course name.
    respondByPolicy({ merge_safe: { conflicts: [conflict('program'), conflict('lesson', { existingId: 'lesson-1', existingTitle: 'Reading a transect' })], canApply: false } });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    const branch = await screen.findByRole('group', { name: /This course is already on this computer/ });
    expect(within(branch).getByRole('listitem')).toHaveTextContent(/^Lesson “Reading a transect” already belongs to another course on this computer\.$/);
    expect(within(branch).getAllByRole('button').map((button) => button.getAttribute('aria-label'))).toEqual([label('create_copy')]);
  });

  it('blocks a pack from a different build with no Apply and no other choice', async () => {
    const user = userEvent.setup();
    const schemaReason = 'This pack was made by a different build of Lattice (database schema 20261001000000; this computer has 20261013000000). Install the same Lattice version on both computers, then export the course again.';
    respondByPolicy({ merge_safe: { conflicts: [conflict('pack_schema', { existingId: null, existingTitle: null, reason: schemaReason })], canApply: false } });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    const branch = await screen.findByRole('group', { name: 'This file cannot be imported as it is' });
    expect(branch).toHaveTextContent(schemaReason);
    expect(branch).toHaveTextContent('None of the import choices can bring this file in.');
    expect(within(branch).queryByRole('button')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Apply these changes' })).not.toBeInTheDocument();
    expect(screen.getByText('Blocked by “Move it here”')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Cancel preview' })).toBeEnabled();
  });

  it('offers a copy, or a fresh check once the course here has finished preparing', async () => {
    const user = userEvent.setup();
    const preparing = 'This course is still being prepared. Export or replace it after its lessons finish generating.';
    let replaceCalls = 0;
    mocks.preview.mockImplementation(async (request: { previewId: string; sourcePath: string; conflictPolicy: string }) => {
      if (request.conflictPolicy === 'merge_safe') return ok(workspace('course-1', [previewOf(request, { conflicts: [conflict('program')], canApply: false })]));
      replaceCalls += 1;
      return ok(workspace('course-1', [previewOf(request, replaceCalls === 1 ? { conflicts: [conflict('program_jobs', { reason: preparing })], canApply: false } : {})]));
    });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: label('replace_after_backup') }));
    const branch = await screen.findByRole('group', { name: 'This file cannot be imported as it is' });
    expect(branch).toHaveTextContent(preparing);
    expect(within(branch).getAllByRole('button').map((button) => button.getAttribute('aria-label') ?? button.textContent?.trim())).toEqual([label('create_copy'), 'Check again']);
    expect(screen.queryByRole('button', { name: 'Apply these changes' })).not.toBeInTheDocument();
    const waiting = mocks.preview.mock.calls[1][0];

    await user.click(within(branch).getByRole('button', { name: 'Check again' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(3));
    expect(mocks.cancel).toHaveBeenLastCalledWith({ previewId: waiting.previewId, operationId: expect.any(String) });
    const again = mocks.preview.mock.calls[2][0];
    expect(again).toMatchObject({ sourcePath: waiting.sourcePath, conflictPolicy: 'replace_after_backup' });
    expect(again.previewId).not.toBe(waiting.previewId);
    expect(again.operationId).not.toBe(waiting.operationId);
    expect(await screen.findByRole('button', { name: 'Apply these changes' })).toBeEnabled();
    expect(screen.queryByRole('group')).not.toBeInTheDocument();
  });

  it('falls back to a copy when the course here has history this file cannot restore', async () => {
    const user = userEvent.setup();
    respondByPolicy({
      merge_safe: { conflicts: [conflict('program')], canApply: false },
      replace_after_backup: { conflicts: [conflict('program_history', { reason: 'Replace was blocked because the target has hidden practical evaluators that this pack version cannot restore.' })], canApply: false },
    });
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: label('replace_after_backup') }));
    const branch = await screen.findByRole('group', { name: 'This file cannot be imported as it is' });
    expect(branch).toHaveTextContent('Replace was blocked because the target has hidden practical evaluators that this pack version cannot restore.');
    expect(within(branch).getAllByRole('button').map((button) => button.getAttribute('aria-label'))).toEqual([label('create_copy')]);
    await user.click(within(branch).getByRole('button', { name: label('create_copy') }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(3));
    expect(mocks.cancel).toHaveBeenCalledTimes(2);
    expect(mocks.preview.mock.calls[2][0]).toMatchObject({ conflictPolicy: 'create_copy' });
  });

  it('keeps the blocked preview when its cancel fails and retries that cancel with the same operation', async () => {
    const user = userEvent.setup();
    respondByPolicy({ merge_safe: { conflicts: [conflict('program')], canApply: false } });
    mocks.cancel.mockResolvedValueOnce(fail('Database is busy'));
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: label('replace_after_backup') }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Database is busy');
    expect(mocks.preview).toHaveBeenCalledTimes(1);
    const firstCancel = mocks.cancel.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: label('replace_after_backup') }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(2));
    expect(mocks.cancel.mock.calls[1][0]).toEqual(firstCancel);
  });

  it('retries a lost preview with the same identity and starts a fresh preview after success', async () => {
    const user = userEvent.setup();
    mocks.preview.mockResolvedValueOnce(fail('Pack read was interrupted'));
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Pack read was interrupted');
    const original = mocks.preview.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry same preview' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(2));
    expect(mocks.preview.mock.calls[1][0]).toEqual(original);
    await screen.findByText('Dry run · pending');
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalledTimes(3));
    expect(mocks.preview.mock.calls[2][0].previewId).not.toBe(original.previewId);
  });

  it('cancels a pending preview and does nothing when the file dialog is closed', async () => {
    const user = userEvent.setup();
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: 'Cancel preview' }));
    await waitFor(() => expect(mocks.cancel).toHaveBeenCalledWith({ previewId: mocks.preview.mock.calls[0][0].previewId, operationId: expect.any(String) }));
    expect(await screen.findByRole('status')).toHaveTextContent('Preview cancelled. Nothing was imported.');
    expect(screen.queryByText(/Dry run/)).not.toBeInTheDocument();

    mocks.open.mockResolvedValueOnce(null);
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    expect(mocks.preview).toHaveBeenCalledTimes(1);
    mocks.open.mockRejectedValueOnce(new Error('Dialog unavailable'));
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Dialog unavailable');
    expect(screen.queryByRole('button', { name: /Retry/ })).not.toBeInTheDocument();
  });

  it('keeps reviewing on request and retries a failed apply with the same operation', async () => {
    const user = userEvent.setup();
    mocks.apply.mockResolvedValueOnce(fail('Import interrupted'));
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: 'Apply these changes' }));
    await user.click(screen.getByRole('button', { name: 'Keep reviewing' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Apply these changes' }));
    await user.click(screen.getByRole('button', { name: 'Confirm import' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Import interrupted');
    expect(mocks.onImported).not.toHaveBeenCalled();
    const first = mocks.apply.mock.calls[0][0];
    await user.click(screen.getByRole('button', { name: 'Retry apply' }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledTimes(2));
    expect(mocks.apply.mock.calls[1][0]).toEqual(first);
    await waitFor(() => expect(mocks.onImported).toHaveBeenCalledTimes(1));
  });

  it('keeps the confirmation open, showing Applying…, until the import settles', async () => {
    const user = userEvent.setup();
    const applying = deferred<ReturnType<typeof ok>>();
    mocks.apply.mockReturnValueOnce(applying.promise);
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: 'Apply these changes' }));
    await user.click(screen.getByRole('button', { name: 'Confirm import' }));
    const confirm = screen.getByRole('dialog', { name: 'Apply the reviewed import?' });
    expect(await within(confirm).findByRole('button', { name: 'Applying…' })).toBeDisabled();
    expect(within(confirm).getByRole('button', { name: 'Keep reviewing' })).toBeDisabled();
    applying.resolve(ok(workspace('course-1', [])));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.getByRole('status')).toHaveTextContent('“Field biology” is ready on this computer.');
    expect(mocks.onImported).toHaveBeenCalledWith('course-1', 'Field biology');
  });

  it('refuses to import when pending work cannot be saved first', async () => {
    const user = userEvent.setup();
    mocks.flush.mockResolvedValue(false);
    renderFlow();
    await user.click(screen.getByRole('button', { name: 'Choose a course file…' }));
    await user.click(await screen.findByRole('button', { name: 'Apply these changes' }));
    await user.click(screen.getByRole('button', { name: 'Confirm import' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be saved before importing');
    expect(mocks.apply).not.toHaveBeenCalled();
  });
});
