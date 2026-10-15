import { useMemo } from 'react';

import VaultAPI from '@/lib/api';
import { toast } from '@/stores/toastStore';

import {
  useCreateBackupMutation,
  useExportMutation,
  type ExportFormat,
} from './useBackupsQuery';

/**
 * Local backup and export, each reported by a toast whose "Show" action opens
 * the folder the files were written to.
 */
export function useBackupAndExport() {
  const createBackup = useCreateBackupMutation();
  const exportData = useExportMutation();

  const runBackup = useMemo(
    () => () => {
      createBackup.mutate(undefined, {
        onSuccess: (result) => {
          toast.success('Backup created', {
            message: 'Saved to your backups folder.',
            action: {
              label: 'Show',
              onClick: () => void VaultAPI.showInFolder(result.backupPath),
            },
          });
        },
        onError: (error) => {
          toast.error("Couldn't back up", { message: error.message });
        },
      });
    },
    [createBackup],
  );

  const runExport = useMemo(
    () => (exportFormat: ExportFormat) => {
      exportData.mutate(exportFormat, {
        onSuccess: (summary) => {
          if (summary.count === 0) {
            toast.info('Nothing to export yet.');
            return;
          }
          toast.success(
            `Exported ${summary.count} ${summary.count === 1 ? 'item' : 'items'}`,
            {
              message:
                exportFormat === 'markdown'
                  ? 'Markdown files are in your exports folder.'
                  : 'The JSON file is in your exports folder.',
              action: {
                label: 'Show',
                onClick: () => void VaultAPI.showInFolder(summary.outputDir),
              },
            },
          );
        },
        onError: (error) => {
          toast.error("Couldn't export", { message: error.message });
        },
      });
    },
    [exportData],
  );

  const showInFolder = (path: string) => VaultAPI.showInFolder(path);

  return { createBackup, exportData, runBackup, runExport, showInFolder };
}
