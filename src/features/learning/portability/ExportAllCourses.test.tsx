import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ExportAllCourses } from '@/features/learning/portability/ExportAllCourses';

const mocks = vi.hoisted(() => ({ exportAll: vi.fn(), showInFolder: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { exportAllLearningPacks: mocks.exportAll, showInFolder: mocks.showInFolder } }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const result = (overrides: Record<string, unknown> = {}) => ({
  cancelled: false,
  destinationDir: '/Volumes/USB/Lattice courses',
  exported: [
    { programId: 'course-1', title: 'Field biology', path: '/Volumes/USB/Lattice courses/Field biology.lattice-learning' },
    { programId: 'course-2', title: 'Draft: Rust ownership', path: '/Volumes/USB/Lattice courses/Draft Rust ownership.lattice-learning' },
  ],
  failed: [{ programId: 'course-3', title: 'Statistics', error: 'Statistics is still being prepared.' }],
  ...overrides,
});

function renderExport() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { invalidate, ...render(<ExportAllCourses />, { wrapper }) };
}

describe('Exporting every course', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.exportAll.mockResolvedValue(ok(result()));
    mocks.showInFolder.mockResolvedValue(ok(null));
  });

  it('exports with the move defaults and lists each saved course and each failure with its reason', async () => {
    const user = userEvent.setup();
    const { invalidate } = renderExport();
    expect(screen.getByRole('heading', { level: 2, name: 'Export all courses' })).toBeVisible();
    expect(screen.getByText('Lesson notes and unsaved lab drafts stay on this computer. They are not part of the files.')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Choose folder and export' }));
    await waitFor(() => expect(mocks.exportAll).toHaveBeenCalledTimes(1));
    expect(mocks.exportAll.mock.calls[0][0]).toEqual({ operationId: expect.any(String), includeEvidence: true, includePracticalArtifacts: true, includeSourceBodies: false, sourceBodyRedistributionConfirmed: false });
    expect(await screen.findByRole('status')).toHaveTextContent('Exported 2 of 3 courses to /Volumes/USB/Lattice courses.');
    const saved = screen.getByRole('list', { name: 'Exported courses' });
    expect(within(saved).getAllByRole('listitem')).toHaveLength(2);
    expect(saved).toHaveTextContent('/Volumes/USB/Lattice courses/Field biology.lattice-learning');
    const failed = screen.getByRole('list', { name: 'Courses that could not be exported' });
    expect(failed).toHaveTextContent('Statistics');
    expect(failed).toHaveTextContent('Statistics is still being prepared.');
    await user.click(screen.getByRole('button', { name: 'Reveal Field biology in folder' }));
    expect(mocks.showInFolder).toHaveBeenCalledWith('/Volumes/USB/Lattice courses/Field biology.lattice-learning');
    expect(invalidate).toHaveBeenCalledWith(expect.objectContaining({ queryKey: ['learning-portability'] }));
  });

  it('treats a closed folder dialog as a quiet no-op', async () => {
    const user = userEvent.setup();
    mocks.exportAll.mockResolvedValue(ok({ cancelled: true, destinationDir: null, exported: [], failed: [] }));
    const { invalidate } = renderExport();
    await user.click(screen.getByRole('button', { name: 'Choose folder and export' }));
    await waitFor(() => expect(mocks.exportAll).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Choose folder and export' })).toBeEnabled());
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('says so when there are no courses to export', async () => {
    const user = userEvent.setup();
    mocks.exportAll.mockResolvedValue(ok(result({ exported: [], failed: [] })));
    renderExport();
    await user.click(screen.getByRole('button', { name: 'Choose folder and export' }));
    expect(await screen.findByRole('status')).toHaveTextContent('There are no courses to export yet');
  });

  it('retries a failed batch with the same operation and reports a reveal failure', async () => {
    const user = userEvent.setup();
    mocks.exportAll.mockResolvedValueOnce(fail('Connection interrupted')).mockResolvedValueOnce(ok(result({ exported: [result().exported[0]], failed: [], destinationDir: null })));
    mocks.showInFolder.mockResolvedValue(fail('The file was moved'));
    renderExport();
    await user.click(screen.getByRole('button', { name: 'Choose folder and export' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    await user.click(screen.getByRole('button', { name: 'Retry same export' }));
    await waitFor(() => expect(mocks.exportAll).toHaveBeenCalledTimes(2));
    expect(mocks.exportAll.mock.calls[1][0]).toEqual(mocks.exportAll.mock.calls[0][0]);
    expect(await screen.findByRole('status')).toHaveTextContent('Exported 1 of 1 course.');
    await user.click(screen.getByRole('button', { name: 'Reveal Field biology in folder' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The file was moved');
  });

  it('requires the redistribution tick before exporting full source text', async () => {
    const user = userEvent.setup();
    renderExport();
    await user.click(screen.getByLabelText(/Include full source text/));
    expect(screen.getByRole('button', { name: 'Choose folder and export' })).toBeDisabled();
    await user.click(screen.getByLabelText(/I confirm that full source bodies/));
    await user.click(screen.getByRole('button', { name: 'Choose folder and export' }));
    await waitFor(() => expect(mocks.exportAll).toHaveBeenCalledWith(expect.objectContaining({ includeSourceBodies: true, sourceBodyRedistributionConfirmed: true })));
    await user.click(screen.getByLabelText(/Include full source text/));
    expect(screen.queryByLabelText(/I confirm that full source bodies/)).not.toBeInTheDocument();
  });
});
