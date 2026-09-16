import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';


import type {
  ArchiveStatus,
  BackupInfo,
  RestoreArchiveResult,
} from '@/types/api/backup';
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
  mockGetArchiveStatus,
  mockBeginArchiveSetup,
  mockConfirmArchiveSetup,
  mockChooseArchiveDestination,
  mockSetArchiveKeepCount,
  mockSetArchivePassphrase,
  mockRotateRecoveryCode,
  mockDisableArchive,
  mockCreateArchiveNow,
  mockRestoreArchive,
} = vi.hoisted(() => ({
  mockListBackups: vi.fn(),
  mockCreateBackup: vi.fn(),
  mockRestoreBackup: vi.fn(),
  mockExportMarkdown: vi.fn(),
  mockExportJson: vi.fn(),
  mockShowInFolder: vi.fn(),
  mockGetIndexProgress: vi.fn(),
  mockGetArchiveStatus: vi.fn(),
  mockBeginArchiveSetup: vi.fn(),
  mockConfirmArchiveSetup: vi.fn(),
  mockChooseArchiveDestination: vi.fn(),
  mockSetArchiveKeepCount: vi.fn(),
  mockSetArchivePassphrase: vi.fn(),
  mockRotateRecoveryCode: vi.fn(),
  mockDisableArchive: vi.fn(),
  mockCreateArchiveNow: vi.fn(),
  mockRestoreArchive: vi.fn(),
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
    getArchiveStatus: mockGetArchiveStatus,
    beginArchiveSetup: mockBeginArchiveSetup,
    confirmArchiveSetup: mockConfirmArchiveSetup,
    chooseArchiveDestination: mockChooseArchiveDestination,
    setArchiveKeepCount: mockSetArchiveKeepCount,
    setArchivePassphrase: mockSetArchivePassphrase,
    rotateRecoveryCode: mockRotateRecoveryCode,
    disableArchive: mockDisableArchive,
    createArchiveNow: mockCreateArchiveNow,
    restoreArchive: mockRestoreArchive,
  },
}));

/** 24 distinct stand-ins for the BIP-39 words. */
const RECOVERY_WORDS = [
  'abandon',
  'ability',
  'able',
  'about',
  'above',
  'absent',
  'absorb',
  'abstract',
  'absurd',
  'abuse',
  'access',
  'accident',
  'account',
  'accuse',
  'achieve',
  'acid',
  'acoustic',
  'acquire',
  'across',
  'act',
  'action',
  'actor',
  'actress',
  'actual',
];

const CONFIRM_INDICES = [2, 10, 17];

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

function archiveStatus(overrides: Partial<ArchiveStatus> = {}): ArchiveStatus {
  return {
    configured: false,
    destination: null,
    destinationProvider: null,
    destinationMissing: false,
    keepCount: 5,
    hasPassphrase: false,
    recoveryConfirmed: false,
    lastSuccess: null,
    lastError: null,
    dataDirCloudProvider: null,
    archives: [],
    ...overrides,
  };
}

function configuredStatus(overrides: Partial<ArchiveStatus> = {}): ArchiveStatus {
  return archiveStatus({
    configured: true,
    destination: '/Users/x/Library/Mobile Documents/com~apple~CloudDocs',
    destinationProvider: 'iCloud Drive',
    recoveryConfirmed: true,
    lastSuccess: {
      path: '/Users/x/Lattice Backups/lattice-backup-20260915.lattice-backup',
      createdAt: '2026-09-15T12:00:00.000Z',
      size: 104857600,
      durationMs: 4500,
    },
    archives: [
      {
        path: '/Users/x/Lattice Backups/lattice-backup-20260915.lattice-backup',
        name: 'lattice-backup-20260915.lattice-backup',
        createdAt: '2026-09-15T12:00:00.000Z',
        size: 104857600,
        availability: 'local',
      },
      {
        path: '/Users/x/Lattice Backups/lattice-backup-20260914.lattice-backup',
        name: 'lattice-backup-20260914.lattice-backup',
        createdAt: '2026-09-14T12:00:00.000Z',
        size: 103809024,
        availability: 'placeholder',
      },
    ],
    ...overrides,
  });
}

function restoreResult(overrides: Partial<RestoreArchiveResult> = {}): RestoreArchiveResult {
  return {
    outcome: 'restored',
    message: null,
    restartRequired: true,
    reembedRequired: true,
    vaultRestoredTo: null,
    filesRestored: 42,
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

/** Walks the wizard from the section's CTA to the word-confirmation step. */
async function openWizardToConfirmStep(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('button', { name: 'Set up encrypted backup' }));
  await user.click(await screen.findByRole('button', { name: 'Show my recovery code' }));
  await screen.findByText(RECOVERY_WORDS[0]);
  await user.click(screen.getByLabelText('I have saved these words somewhere safe'));
  await user.click(screen.getByRole('button', { name: 'Next' }));
  await screen.findByLabelText('Word 3');
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

    mockGetArchiveStatus.mockResolvedValue({ ok: true, data: archiveStatus() });
    mockBeginArchiveSetup.mockResolvedValue({
      ok: true,
      data: { recoveryWords: RECOVERY_WORDS, confirmIndices: CONFIRM_INDICES },
    });
    mockRotateRecoveryCode.mockResolvedValue({
      ok: true,
      data: { recoveryWords: RECOVERY_WORDS, confirmIndices: CONFIRM_INDICES },
    });
    mockConfirmArchiveSetup.mockResolvedValue({ ok: true, data: configuredStatus() });
    mockChooseArchiveDestination.mockResolvedValue({ ok: true, data: configuredStatus() });
    mockSetArchiveKeepCount.mockResolvedValue({ ok: true, data: configuredStatus() });
    mockSetArchivePassphrase.mockResolvedValue({
      ok: true,
      data: configuredStatus({ hasPassphrase: true }),
    });
    mockDisableArchive.mockResolvedValue({
      ok: true,
      data: configuredStatus({ destination: null, destinationProvider: null }),
    });
    mockCreateArchiveNow.mockResolvedValue({
      ok: true,
      data: {
        path: '/Users/x/Lattice Backups/lattice-backup-20260916.lattice-backup',
        createdAt: '2026-09-16T12:00:00.000Z',
        size: 104857600,
        durationMs: 4200,
      },
    });
    mockRestoreArchive.mockResolvedValue({ ok: true, data: restoreResult({ outcome: 'cancelled' }) });
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

  describe('off-device backup', () => {
    it('offers setup when nothing is configured', async () => {
      renderSection();

      expect(await screen.findByText('Off-device backup')).toBeInTheDocument();
      expect(
        screen.getByRole('button', { name: 'Set up encrypted backup' }),
      ).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Restore from file' })).not.toBeInTheDocument();
    });

    it('confirms the three words the backend asked for and no passphrase', async () => {
      const user = userEvent.setup();
      renderSection();

      await openWizardToConfirmStep(user);

      await user.type(screen.getByLabelText('Word 3'), RECOVERY_WORDS[2]);
      await user.type(screen.getByLabelText('Word 11'), RECOVERY_WORDS[10]);
      await user.type(screen.getByLabelText('Word 18'), RECOVERY_WORDS[17]);
      await user.click(screen.getByRole('button', { name: 'Confirm' }));

      await waitFor(() =>
        expect(mockConfirmArchiveSetup).toHaveBeenCalledWith(
          [
            { index: 2, word: RECOVERY_WORDS[2] },
            { index: 10, word: RECOVERY_WORDS[10] },
            { index: 17, word: RECOVERY_WORDS[17] },
          ],
          null,
        ),
      );

      // The passphrase is its own step, so it cannot have been sent yet.
      expect(mockSetArchivePassphrase).not.toHaveBeenCalled();
      expect(await screen.findByLabelText('Passphrase')).toBeInTheDocument();
    });

    it('stays on the confirmation step when a word is wrong', async () => {
      const user = userEvent.setup();
      mockConfirmArchiveSetup.mockResolvedValue({
        ok: false,
        error: "That isn't the right word.",
      });
      renderSection();

      await openWizardToConfirmStep(user);

      await user.type(screen.getByLabelText('Word 3'), 'wrong');
      await user.type(screen.getByLabelText('Word 11'), RECOVERY_WORDS[10]);
      await user.type(screen.getByLabelText('Word 18'), RECOVERY_WORDS[17]);
      await user.click(screen.getByRole('button', { name: 'Confirm' }));

      expect(await screen.findByRole('alert')).toHaveTextContent("That isn't the right word.");
      expect(screen.getByLabelText('Word 3')).toBeInTheDocument();
      expect(screen.queryByLabelText('Passphrase')).not.toBeInTheDocument();
    });

    it('shows the folder, its provider, the last backup, and undownloaded archives', async () => {
      mockGetArchiveStatus.mockResolvedValue({ ok: true, data: configuredStatus() });
      renderSection();

      expect(
        await screen.findByText('/Users/x/Library/Mobile Documents/com~apple~CloudDocs'),
      ).toBeInTheDocument();
      expect(screen.getByText('iCloud Drive')).toBeInTheDocument();
      expect(screen.getByText(/Last backup Sep 15, 2026/)).toBeInTheDocument();
      expect(screen.getByText('not downloaded')).toBeInTheDocument();
    });

    it('asks for the passphrase or recovery code and retries with it', async () => {
      const user = userEvent.setup();
      mockGetArchiveStatus.mockResolvedValue({ ok: true, data: configuredStatus() });
      mockRestoreArchive
        .mockResolvedValueOnce({ ok: true, data: restoreResult({ outcome: 'needs_secret' }) })
        .mockResolvedValueOnce({ ok: true, data: restoreResult() });
      renderSection();

      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));

      const secretField = await screen.findByLabelText('Passphrase or recovery code');
      expect(mockRestoreArchive).toHaveBeenNthCalledWith(1, null);

      await user.type(secretField, 'open sesame');
      await user.click(screen.getByRole('button', { name: 'Unlock' }));

      await waitFor(() => expect(mockRestoreArchive).toHaveBeenNthCalledWith(2, 'open sesame'));
      expect(
        await screen.findByText(/Search indexes rebuild on the next launch/),
      ).toBeInTheDocument();
    });

    it('names the provider when the archive is still a placeholder', async () => {
      const user = userEvent.setup();
      mockGetArchiveStatus.mockResolvedValue({
        ok: true,
        data: configuredStatus({ destinationProvider: 'Dropbox' }),
      });
      mockRestoreArchive.mockResolvedValue({
        ok: true,
        data: restoreResult({ outcome: 'not_hydrated', message: 'iCloud Drive' }),
      });
      renderSection();

      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));

      expect(await screen.findByText(/still only in iCloud Drive/)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    });

    it('asks before turning off', async () => {
      const user = userEvent.setup();
      mockGetArchiveStatus.mockResolvedValue({ ok: true, data: configuredStatus() });
      renderSection();

      await user.click(await screen.findByRole('button', { name: 'Turn off' }));

      expect(screen.getByText('Turn off off-device backup?')).toBeInTheDocument();
      expect(mockDisableArchive).not.toHaveBeenCalled();
    });

    it('warns when the live database sits in a synced folder', async () => {
      mockGetArchiveStatus.mockResolvedValue({
        ok: true,
        data: configuredStatus({ dataDirCloudProvider: 'OneDrive' }),
      });
      renderSection();

      expect(
        await screen.findByText(/Your Lattice database itself is inside OneDrive/),
      ).toBeInTheDocument();
    });
  });
});
