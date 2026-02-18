import { useState, useEffect } from 'react';

import { appDataDir } from '@tauri-apps/api/path';
import { save, open } from '@tauri-apps/plugin-dialog';

import VaultAPI from '@/lib/api';
import type { BackupInfo } from '@/types/api/backup';

import { handleTauriError } from '../../utils/errorHandler';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { ConfirmDialog } from '../ConfirmDialog';


export function BackupPanel() {
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [autoBackupEnabled, setAutoBackupEnabled] = useState(false);
  const [schedule, setSchedule] = useState('daily');
  const [restoreCandidate, setRestoreCandidate] = useState<string | null>(null);

  useEffect(() => {
    void loadBackups();
  }, []);

  const loadBackups = async () => {
    try {
      const dataDir = await appDataDir();
      const result = await VaultAPI.listBackups(dataDir);
      if (result.ok) {
        setBackups(result.data);
        setError(null);
      } else {
        throw new Error(result.error);
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Failed to load backups:', err);
      setError(errorMessage);
    }
  };

  const handleCreateBackup = async () => {
    setLoading(true);
    setStatus('Creating backup...');
    setError(null);
    try {
      const savePath = await save({
        filters: [
          {
            name: 'Vault Backup',
            extensions: ['vault-backup'],
          },
        ],
        defaultPath: `backup_${new Date().toISOString().split('T')[0]}.vault-backup`,
      });

      if (savePath) {
        const result = await VaultAPI.createBackup(savePath);
        if (result.ok) {
          setStatus(`Backup created: ${result.data}`);
          await loadBackups();
        } else {
          throw new Error(result.error);
        }
      } else {
        setStatus('Backup cancelled');
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Backup creation failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleRestore = async (backupPath?: string) => {
    setError(null);
    try {
      const selected =
        backupPath ||
        (await open({
          filters: [
            {
              name: 'Vault Backup',
              extensions: ['vault-backup'],
            },
          ],
          multiple: false,
        }));

      if (selected) {
        const path = Array.isArray(selected) ? selected[0] : selected;
        setRestoreCandidate(path);
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Restore failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const confirmRestore = async () => {
    if (!restoreCandidate) return;

    setLoading(true);
    setStatus('Restoring backup...');
    try {
      const result = await VaultAPI.restoreBackup(restoreCandidate);
      if (result.ok) {
        setStatus('Restore complete!');
      } else {
        throw new Error(result.error);
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Restore failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
      setRestoreCandidate(null);
    }
  };

  const handleExportMarkdown = async () => {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        setLoading(true);
        setStatus('Exporting to Markdown...');
        const result = await VaultAPI.exportMarkdown(dir);
        if (result.ok) {
          setStatus(`Exported ${result.data} files to Markdown`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Export failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleExportJSON = async () => {
    setError(null);
    try {
      const path = await save({
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
        defaultPath: 'recall_export.json',
      });

      if (path) {
        setLoading(true);
        setStatus('Exporting to JSON...');
        const result = await VaultAPI.exportJson(path, true);
        if (result.ok) {
          setStatus('Exported to JSON');
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Export failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleExportCSV = async () => {
    setError(null);
    try {
      const path = await save({
        filters: [
          {
            name: 'CSV',
            extensions: ['csv'],
          },
        ],
        defaultPath: 'recall_export.csv',
      });

      if (path) {
        setLoading(true);
        setStatus('Exporting to CSV...');
        const result = await VaultAPI.exportCsv(path);
        if (result.ok) {
          setStatus(`Exported ${result.data} files to CSV`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Export failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleExportHTML = async () => {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        setLoading(true);
        setStatus('Exporting to HTML...');
        const result = await VaultAPI.exportHtml(dir);
        if (result.ok) {
          setStatus(`Exported ${result.data} files to HTML`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Export failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleImportObsidian = async () => {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        setLoading(true);
        setStatus('Importing Obsidian vault...');
        const result = await VaultAPI.importObsidianVault(dir);
        if (result.ok) {
          setStatus(`Imported ${result.data} files from Obsidian`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Import failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleImportNotion = async () => {
    setError(null);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (dir) {
        setLoading(true);
        setStatus('Importing Notion export...');
        const result = await VaultAPI.importNotionExport(dir);
        if (result.ok) {
          setStatus(`Imported ${result.data} files from Notion`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Import failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleImportRoam = async () => {
    setError(null);
    try {
      const file = await open({
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
        multiple: false,
      });

      if (file) {
        const path = Array.isArray(file) ? file[0] : file;
        setLoading(true);
        setStatus('Importing Roam Research export...');
        const result = await VaultAPI.importRoamJson(path);
        if (result.ok) {
          setStatus(`Imported ${result.data} pages from Roam Research`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Import failed:', err);
      setStatus('');
      setError(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const toggleAutoBackup = async () => {
    setError(null);
    try {
      if (autoBackupEnabled) {
        const result = await VaultAPI.stopAutoBackup();
        if (result.ok) {
          setAutoBackupEnabled(false);
          setStatus('Auto-backup stopped');
        } else {
          throw new Error(result.error);
        }
      } else {
        const result = await VaultAPI.startAutoBackup(schedule);
        if (result.ok) {
          setAutoBackupEnabled(true);
          setStatus(`Auto-backup started (${schedule})`);
        } else {
          throw new Error(result.error);
        }
      }
    } catch (err) {
      const errorMessage = handleTauriError(err);
      console.error('Auto-backup toggle failed:', err);
      setStatus('');
      setError(errorMessage);
    }
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  return (
    <div className="backup-panel p-6 max-w-4xl mx-auto">
      <h2 className="text-2xl font-bold mb-6">Backup & Export</h2>

      {error && (
        <div className="mb-4 p-3 bg-[var(--error-light)]/20 text-[var(--error)] border border-[var(--error-light)] rounded">
          {error}
        </div>
      )}

      {status && (
        <div className="mb-4 p-3 bg-[var(--accent-light)] border border-[var(--accent-light)] rounded">
          {status}
        </div>
      )}

      <div className="space-y-6">
        <div className="card bg-[var(--surface-elevated)] shadow rounded-lg p-6">
          <h3 className="font-semibold text-lg mb-2">Create Backup</h3>
          <p className="text-sm text-[var(--text-secondary)] mb-4">
            Save all your data, embeddings, and settings to a backup file
          </p>
          <button
            onClick={handleAsyncEvent(handleCreateBackup)}
            disabled={loading}
            className="btn bg-[var(--accent-primary)] text-white px-4 py-2 rounded hover:bg-[var(--accent-hover)] disabled:opacity-50"
          >
            {loading ? 'Creating...' : 'Create Backup'}
          </button>
        </div>

        <div className="card bg-[var(--surface-elevated)] shadow rounded-lg p-6">
          <h3 className="font-semibold text-lg mb-2">Auto Backup</h3>
          <p className="text-sm text-[var(--text-secondary)] mb-4">
            Automatically create backups on a schedule
          </p>
          <div className="flex items-center gap-4">
            <select
              value={schedule}
              onChange={(e) => setSchedule(e.target.value)}
              className="border rounded px-3 py-2"
              disabled={autoBackupEnabled}
            >
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="monthly">Monthly</option>
            </select>
            <button
              onClick={handleAsyncEvent(toggleAutoBackup)}
              className={`btn px-4 py-2 rounded ${
                autoBackupEnabled
                  ? 'bg-[var(--error)] text-white hover:bg-[var(--error)]'
                  : 'bg-[var(--success)] text-white hover:bg-[var(--success)]'
              }`}
            >
              {autoBackupEnabled ? 'Stop Auto Backup' : 'Start Auto Backup'}
            </button>
          </div>
        </div>

        <div className="card bg-[var(--surface-elevated)] shadow rounded-lg p-6">
          <h3 className="font-semibold text-lg mb-2">Restore Backup</h3>
          <p className="text-sm text-[var(--text-secondary)] mb-4">
            Replace current data with a backup
          </p>
          <button
            onClick={handleAsyncEvent(async () => handleRestore())}
            disabled={loading}
            className="btn bg-[var(--warning)] text-white px-4 py-2 rounded hover:bg-[var(--warning)]/80 disabled:opacity-50"
          >
            Restore from Backup
          </button>

          {backups.length > 0 && (
            <div className="mt-4">
              <h4 className="font-medium mb-2">Available Backups</h4>
              <div className="space-y-2">
                {backups.map((backup) => (
                  <div
                    key={backup.path}
                    className="flex items-center justify-between p-3 border rounded hover:bg-[var(--bg-secondary)]"
                  >
                    <div>
                      <div className="font-medium">{backup.name}</div>
                      <div className="text-xs text-[var(--text-tertiary)]">
                        {backup.createdAt} • {formatFileSize(backup.size)}
                      </div>
                    </div>
                    <button
                      onClick={handleAsyncEvent(async () => handleRestore(backup.path))}
                      className="btn bg-[var(--text-secondary)] text-white px-3 py-1 rounded text-sm hover:bg-[var(--surface-elevated)]"
                    >
                      Restore
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="card bg-[var(--surface-elevated)] shadow rounded-lg p-6">
          <h3 className="font-semibold text-lg mb-2">Export</h3>
          <p className="text-sm text-[var(--text-secondary)] mb-4">
            Export your data in various formats
          </p>
          <div className="grid grid-cols-2 gap-3">
            <button
              onClick={handleAsyncEvent(handleExportMarkdown)}
              disabled={loading}
              className="btn border border-[var(--border-color)] px-4 py-2 rounded hover:bg-[var(--bg-secondary)] disabled:opacity-50"
            >
              Export as Markdown
            </button>
            <button
              onClick={handleAsyncEvent(handleExportJSON)}
              disabled={loading}
              className="btn border border-[var(--border-color)] px-4 py-2 rounded hover:bg-[var(--bg-secondary)] disabled:opacity-50"
            >
              Export as JSON
            </button>
            <button
              onClick={handleAsyncEvent(handleExportCSV)}
              disabled={loading}
              className="btn border border-[var(--border-color)] px-4 py-2 rounded hover:bg-[var(--bg-secondary)] disabled:opacity-50"
            >
              Export as CSV
            </button>
            <button
              onClick={handleAsyncEvent(handleExportHTML)}
              disabled={loading}
              className="btn border border-[var(--border-color)] px-4 py-2 rounded hover:bg-[var(--bg-secondary)] disabled:opacity-50"
            >
              Export as HTML
            </button>
          </div>
        </div>

        <div className="card bg-[var(--surface-elevated)] shadow rounded-lg p-6">
          <h3 className="font-semibold text-lg mb-2">Import</h3>
          <p className="text-sm text-[var(--text-secondary)] mb-4">
            Import data from other applications
          </p>
          <div className="grid grid-cols-2 gap-3">
            <button
              onClick={handleAsyncEvent(handleImportObsidian)}
              disabled={loading}
              className="btn border border-[var(--warning)] px-4 py-2 rounded hover:bg-[var(--warning-light)]/20 disabled:opacity-50"
            >
              Import Obsidian Vault
            </button>
            <button
              onClick={handleAsyncEvent(handleImportNotion)}
              disabled={loading}
              className="btn border border-[var(--border-color)] px-4 py-2 rounded hover:bg-[var(--bg-secondary)] disabled:opacity-50"
            >
              Import Notion Export
            </button>
            <button
              onClick={handleAsyncEvent(handleImportRoam)}
              disabled={loading}
              className="btn border border-[var(--accent-light)] px-4 py-2 rounded hover:bg-[var(--accent-light)] disabled:opacity-50"
            >
              Import Roam Research
            </button>
          </div>
        </div>
      </div>

      <ConfirmDialog
        isOpen={restoreCandidate !== null}
        title="Restore Backup"
        message="This will replace all current data. Continue?"
        confirmLabel="Restore"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={loading}
        onConfirm={confirmRestore}
        onCancel={() => setRestoreCandidate(null)}
      />
    </div>
  );
}
