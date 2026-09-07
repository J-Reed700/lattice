import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';


import type { BackupInfo } from '@/types/api/backup';
import type { IndexingSnapshot } from '@/types/api/indexing';

import { BackupSection } from './BackupSection';

const {
  mockListBackups,
  mockCreateBackup,
  mockRestoreBackup,
  mockExportMarkdown,
  mockExportJson,
  mockShowInFolder,
  mockGetIndexProgress,
} = vi.hoisted(() => ({
  mockListBackups: vi.fn(),
  mockCreateBackup: vi.fn(),
  mockRestoreBackup: vi.fn(),
  mockExportMarkdown: vi.fn(),
  mockExportJson: vi.fn(),
  mockShowInFolder: vi.fn(),
  mockGetIndexProgress: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  __esModule: true,
  default: {
    listBackups: mockListBackups,
    createBackup: mockCreateBackup,
    restoreBackup: mockRestoreBackup,
    exportMarkdown: mockExportMarkdown,
    exportJson: mockExportJson,
    showInFolder: mockShowInFolder,
    getIndexProgress: mockGetIndexProgress,
  },
}));

function backup(overrides: Partial<BackupInfo> = {}): BackupInfo {
  return {
    path: '/data/backups/lattice-20260906-140200.db',
    name: 'lattice-20260906-140200.db',
    createdAt: '2026-09-06T14:02:00.000Z',
    version: '1.0',
    fileCount: 1247,
    size: 432013312,
    ...overrides,
  };
}

function snapshot(overrides: Partial<IndexingSnapshot> = {}): IndexingSnapshot {
  return {
    totalFiles: 0,
    processed: 0,
    failed: 0,
    status: 'idle',
    percentage: 0,
    paused: false,
    failures: [],
    ...overrides,
  };
}

function renderSection() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<BackupSection />, { wrapper });
}

describe('BackupSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListBackups.mockResolvedValue({ ok: true, data: [] });
    mockCreateBackup.mockResolvedValue({
      ok: true,
      data: { backupPath: '/data/backups/new.db', size: 1, createdAt: '2026-09-06T14:02:00.000Z' },
    });
    mockRestoreBackup.mockResolvedValue({
      ok: true,
      data: { success: true, restoredCount: 0, message: null },
    });
    mockExportMarkdown.mockResolvedValue({
      ok: true,
      data: { count: 12, outputDir: '/data/exports/lattice-export-20260906-140200' },
    });
    mockExportJson.mockResolvedValue({
      ok: true,
      data: { count: 12, outputDir: '/data/exports/lattice-export-20260906-140200' },
    });
    mockShowInFolder.mockResolvedValue({ ok: true, data: undefined });
    mockGetIndexProgress.mockResolvedValue({ ok: true, data: snapshot() });
  });

  it('says so when there are no backups', async () => {
    renderSection();
    expect(await screen.findByText('No backups yet.')).toBeInTheDocument();
  });

  it('shows the date, the size, and both row actions', async () => {
    mockListBackups.mockResolvedValue({ ok: true, data: [backup()] });
    renderSection();

    expect(await screen.findByText(/1,247 documents · 412 MB/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Restore…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Show' })).toBeInTheDocument();
  });

  it('omits a zero document count rather than printing it', async () => {
    mockListBackups.mockResolvedValue({ ok: true, data: [backup({ fileCount: 0 })] });
    renderSection();

    await screen.findByRole('button', { name: 'Restore…' });
    expect(screen.queryByText(/0 documents/)).not.toBeInTheDocument();
  });

  it('names the backup date in the confirmation and requires typing to confirm', async () => {
    const user = userEvent.setup();
    mockListBackups.mockResolvedValue({ ok: true, data: [backup()] });
    renderSection();

    await user.click(await screen.findByRole('button', { name: 'Restore…' }));

    expect(screen.getByText('Restore this backup?')).toBeInTheDocument();
    expect(
      screen.getByText(/This replaces your current library with the backup from Sep 6, 2026 at/),
    ).toBeInTheDocument();

    const confirmButton = screen.getByRole('button', { name: 'Restore' });
    expect(confirmButton).toBeDisabled();

    await user.type(screen.getByPlaceholderText('restore'), 'restore');
    expect(confirmButton).toBeEnabled();
  });

  it('goes terminal after a successful restore', async () => {
    const user = userEvent.setup();
    mockListBackups.mockResolvedValue({ ok: true, data: [backup()] });
    renderSection();

    await user.click(await screen.findByRole('button', { name: 'Restore…' }));
    await user.type(screen.getByPlaceholderText('restore'), 'restore');
    await user.click(screen.getByRole('button', { name: 'Restore' }));

    expect(
      await screen.findByText('Quit Lattice and open it again to use the restored library.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('disables Restore while a run is indexing', async () => {
    mockListBackups.mockResolvedValue({ ok: true, data: [backup()] });
    mockGetIndexProgress.mockResolvedValue({
      ok: true,
      data: snapshot({ status: 'processing', processed: 3, totalFiles: 10 }),
    });
    renderSection();

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Restore…' })).toBeDisabled(),
    );
    expect(screen.getByRole('button', { name: 'Restore…' })).toHaveAttribute(
      'title',
      'Wait for indexing to finish before restoring.',
    );
  });

  it('exports through the markdown command', async () => {
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByRole('button', { name: 'Markdown' }));

    await waitFor(() => expect(mockExportMarkdown).toHaveBeenCalled());
  });
});
