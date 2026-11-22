import { create } from 'zustand';

interface RestoreLockoutState {
  /**
   * True after `restoreBackup` or `restoreArchive` completes successfully.
   * The backend closes the SQLite connection pool on restore and requires
   * restarting the app, so all views must remain locked out across route changes.
   */
  isDatabaseRestoredRestartRequired: boolean;
  restoredFromArchive: boolean;
  markDatabaseRestoredRestartRequired: (fromArchive?: boolean) => void;
  resetRestoreLockout: () => void;
}

export const useRestoreLockoutStore = create<RestoreLockoutState>((set) => ({
  isDatabaseRestoredRestartRequired: false,
  restoredFromArchive: false,
  markDatabaseRestoredRestartRequired: (fromArchive = false) =>
    set((state) => ({
      isDatabaseRestoredRestartRequired: true,
      restoredFromArchive: state.restoredFromArchive || fromArchive,
    })),
  resetRestoreLockout: () =>
    set({
      isDatabaseRestoredRestartRequired: false,
      restoredFromArchive: false,
    }),
}));
