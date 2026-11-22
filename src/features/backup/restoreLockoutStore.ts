import { create } from 'zustand';

interface RestoreLockoutState {
  /**
   * True after `restoreBackup` or `restoreArchive` completes successfully, or
   * fails after closing the SQLite connection pool.
   * The backend closes the SQLite connection pool on restore and requires
   * restarting the app, so all views must remain locked out across route changes.
   */
  isDatabaseRestoredRestartRequired: boolean;
  restoredFromArchive: boolean;
  restoreFailed: boolean;
  markDatabaseRestoredRestartRequired: (fromArchive?: boolean, restoreFailed?: boolean) => void;
  resetRestoreLockout: () => void;
}

export const useRestoreLockoutStore = create<RestoreLockoutState>((set) => ({
  isDatabaseRestoredRestartRequired: false,
  restoredFromArchive: false,
  restoreFailed: false,
  markDatabaseRestoredRestartRequired: (fromArchive = false, restoreFailed = false) =>
    set((state) => ({
      isDatabaseRestoredRestartRequired: true,
      restoredFromArchive: state.restoredFromArchive || fromArchive,
      restoreFailed: state.restoreFailed || restoreFailed,
    })),
  resetRestoreLockout: () =>
    set({
      isDatabaseRestoredRestartRequired: false,
      restoredFromArchive: false,
      restoreFailed: false,
    }),
}));
