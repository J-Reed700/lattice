import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';


import { usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';
import { useToastStore } from '@/stores/toastStore';
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

  describe('archive actions', () => {
    const toastTitles = () => useToastStore.getState().toasts.map((toast) => toast.title);
    const offDeviceBackUp = async () => {
      // The folder section renders once the archive status arrives.
      await screen.findByRole('button', { name: 'Restore from file' });
      return screen.getAllByRole('button', { name: 'Back up now' })[1];
    };

    beforeEach(() => {
      useToastStore.setState({ toasts: [] });
      mockGetArchiveStatus.mockResolvedValue({ ok: true, data: configuredStatus() });
    });

    it('backs up to the folder and reports the size', async () => {
      const user = userEvent.setup();
      renderSection();
      await user.click(await offDeviceBackUp());
      await waitFor(() => expect(toastTitles()).toContain('Backed up to your folder'));
      expect(useToastStore.getState().toasts[0].message).toBe('One encrypted file, 100 MB.');
    });

    it('reports a backup to the folder that failed, and one with no size', async () => {
      const user = userEvent.setup();
      mockCreateArchiveNow.mockResolvedValueOnce({ ok: false, error: 'folder is read-only' });
      renderSection();
      await user.click(await offDeviceBackUp());
      await waitFor(() => expect(toastTitles()).toContain("Couldn't back up to your folder"));
      mockCreateArchiveNow.mockResolvedValueOnce({ ok: true, data: { path: '/x', createdAt: '2026-09-16T12:00:00.000Z', size: 0, durationMs: 1 } });
      await user.click(await offDeviceBackUp());
      await waitFor(() => expect(useToastStore.getState().toasts[0].message).toBe('One encrypted file.'));
    });

    it('does nothing when the file picker is cancelled', async () => {
      const user = userEvent.setup();
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));
      await waitFor(() => expect(mockRestoreArchive).toHaveBeenCalledWith(null));
      expect(screen.queryByLabelText('Passphrase or recovery code')).not.toBeInTheDocument();
      expect(toastTitles()).toEqual([]);
    });

    it('reports a restore that failed before any secret was asked for', async () => {
      const user = userEvent.setup();
      mockRestoreArchive.mockResolvedValueOnce({ ok: false, error: 'archive is corrupt' });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));
      await waitFor(() => expect(toastTitles()).toContain("Couldn't restore that backup"));
    });

    it('keeps the unlock dialog open on a wrong secret', async () => {
      const user = userEvent.setup();
      mockRestoreArchive
        .mockResolvedValueOnce({ ok: true, data: restoreResult({ outcome: 'needs_secret' }) })
        .mockResolvedValueOnce({ ok: false, error: 'That passphrase does not open this archive.' });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));
      await user.type(await screen.findByLabelText('Passphrase or recovery code'), 'wrong words');
      await user.click(screen.getByRole('button', { name: 'Unlock' }));
      expect(await screen.findByText('That passphrase does not open this archive.')).toBeInTheDocument();
      expect(screen.getByLabelText('Passphrase or recovery code')).toBeInTheDocument();
    });

    it('explains an undownloaded archive without a provider and retries with the last secret', async () => {
      const user = userEvent.setup();
      mockRestoreArchive.mockResolvedValue({ ok: true, data: restoreResult({ outcome: 'not_hydrated', message: null }) });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Restore from file' }));
      expect(await screen.findByText(/hasn't been downloaded to this computer yet\. Download it in your cloud folder/)).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Try again' }));
      await waitFor(() => expect(mockRestoreArchive).toHaveBeenCalledTimes(2));
      expect(mockRestoreArchive).toHaveBeenLastCalledWith(null);
    });

    it('reports a local backup that could not be restored', async () => {
      const user = userEvent.setup();
      mockListBackups.mockResolvedValue({ ok: true, data: [backup()] });
      mockRestoreBackup.mockResolvedValueOnce({ ok: false, error: 'backup file is truncated' });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Restore…' }));
      await user.type(screen.getByPlaceholderText('restore'), 'restore');
      await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: /^Restore/ }));
      await waitFor(() => expect(toastTitles()).toContain("Couldn't restore that backup"));
      expect(screen.queryByText('Restore this backup?')).not.toBeInTheDocument();
    });

    it('changes the folder and how many archives to keep, reporting failures', async () => {
      const user = userEvent.setup();
      mockChooseArchiveDestination.mockResolvedValueOnce({ ok: false, error: 'not writable' });
      mockSetArchiveKeepCount.mockResolvedValueOnce({ ok: false, error: 'out of range' });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Change' }));
      await waitFor(() => expect(toastTitles()).toContain("Couldn't change the folder"));
      await user.selectOptions(screen.getByLabelText('Archives to keep'), '10');
      await waitFor(() => expect(mockSetArchiveKeepCount).toHaveBeenCalledWith(10));
      await waitFor(() => expect(toastTitles()).toContain("Couldn't change how many to keep"));
    });

    describe('passphrase', () => {
      it('checks length and agreement before saving', async () => {
        const user = userEvent.setup();
        renderSection();
        await user.click(await screen.findByRole('button', { name: 'Set passphrase' }));
        expect(screen.getByRole('heading', { name: 'Set a passphrase' })).toBeInTheDocument();
        await user.type(screen.getByLabelText('Passphrase'), 'short');
        await user.click(screen.getByRole('button', { name: 'Save' }));
        expect(screen.getByRole('alert')).toHaveTextContent('The passphrase has to be at least 8 characters.');
        await user.type(screen.getByLabelText('Passphrase'), ' and long');
        await user.type(screen.getByLabelText('Passphrase again'), 'something else');
        await user.click(screen.getByRole('button', { name: 'Save' }));
        expect(screen.getByRole('alert')).toHaveTextContent("The two passphrases don't match.");
        expect(mockSetArchivePassphrase).not.toHaveBeenCalled();
      });

      it('saves a passphrase and closes', async () => {
        const user = userEvent.setup();
        renderSection();
        await user.click(await screen.findByRole('button', { name: 'Set passphrase' }));
        await user.type(screen.getByLabelText('Passphrase'), 'river stone moss');
        await user.type(screen.getByLabelText('Passphrase again'), 'river stone moss');
        await user.click(screen.getByRole('button', { name: 'Save' }));
        await waitFor(() => expect(toastTitles()).toContain('Passphrase saved'));
        expect(mockSetArchivePassphrase).toHaveBeenCalledWith('river stone moss');
        await waitFor(() => expect(screen.queryByRole('heading', { name: 'Set a passphrase' })).not.toBeInTheDocument());
      });

      it('shows a refused passphrase in the dialog', async () => {
        const user = userEvent.setup();
        mockSetArchivePassphrase.mockResolvedValueOnce({ ok: false, error: 'keyring locked' });
        renderSection();
        await user.click(await screen.findByRole('button', { name: 'Set passphrase' }));
        await user.type(screen.getByLabelText('Passphrase'), 'river stone moss');
        await user.type(screen.getByLabelText('Passphrase again'), 'river stone moss');
        await user.click(screen.getByRole('button', { name: 'Save' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('keyring locked');
      });

      it('removes an existing passphrase, or shows why it could not', async () => {
        const user = userEvent.setup();
        mockGetArchiveStatus.mockResolvedValue({ ok: true, data: configuredStatus({ hasPassphrase: true }) });
        mockSetArchivePassphrase.mockResolvedValueOnce({ ok: false, error: 'keyring locked' });
        renderSection();
        await user.click(await screen.findByRole('button', { name: 'Change passphrase' }));
        await user.click(screen.getByRole('button', { name: 'Remove passphrase' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('keyring locked');
        expect(mockSetArchivePassphrase).toHaveBeenCalledWith(null);
        await user.click(screen.getByRole('button', { name: 'Remove passphrase' }));
        await waitFor(() => expect(toastTitles()).toContain('Passphrase removed'));
      });

      it('closes from Cancel', async () => {
        const user = userEvent.setup();
        renderSection();
        await user.click(await screen.findByRole('button', { name: 'Set passphrase' }));
        await user.click(screen.getByRole('button', { name: 'Cancel' }));
        await waitFor(() => expect(screen.queryByRole('heading', { name: 'Set a passphrase' })).not.toBeInTheDocument());
      });
    });

    it('turns off after confirmation', async () => {
      const user = userEvent.setup();
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Turn off' }));
      const dialog = screen.getByRole('dialog');
      await user.click(within(dialog).getByRole('button', { name: /^Turn off/ }));
      await waitFor(() => expect(mockDisableArchive).toHaveBeenCalled());
      await waitFor(() => expect(screen.queryByText('Turn off off-device backup?')).not.toBeInTheDocument());
    });

    it('shows why turning off failed', async () => {
      const user = userEvent.setup();
      vi.spyOn(console, 'error').mockImplementation(() => {});
      mockDisableArchive.mockResolvedValueOnce({ ok: false, error: 'archive busy' });
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Turn off' }));
      await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: /^Turn off/ }));
      expect(await screen.findByText('archive busy')).toBeInTheDocument();
    });

    it('starts a new recovery code', async () => {
      const user = userEvent.setup();
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'Generate new recovery code' }));
      expect(await screen.findByRole('dialog')).toBeInTheDocument();
    });

    it('registers the vault commands in the palette', async () => {
      renderSection();
      await screen.findByRole('button', { name: 'Restore from file' });
      const commands = usePaletteCommandsStore.getState().commands;
      expect(commands.get('vault.archive.backup')?.enabled).toBe(true);
      expect(commands.get('vault.archive.restore')?.enabled).toBe(true);
      await act(async () => { await commands.get('vault.archive.backup')!.run(); });
      await waitFor(() => expect(mockCreateArchiveNow).toHaveBeenCalled());
      await act(async () => { await commands.get('vault.archive.restore')!.run(); });
      await waitFor(() => expect(mockRestoreArchive).toHaveBeenCalledWith(null));
      await act(async () => { await commands.get('vault.export.markdown')!.run(); });
      await waitFor(() => expect(mockExportMarkdown).toHaveBeenCalled());
      await act(async () => { await commands.get('vault.backup')!.run(); });
      await waitFor(() => expect(mockCreateBackup).toHaveBeenCalled());
    });

    it('exports as JSON', async () => {
      const user = userEvent.setup();
      renderSection();
      await user.click(await screen.findByRole('button', { name: 'JSON' }));
      await waitFor(() => expect(mockExportJson).toHaveBeenCalled());
    });
  });
});
