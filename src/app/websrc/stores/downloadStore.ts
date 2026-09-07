import { create } from 'zustand';

/** UI-only state for the global downloads surface. */
interface DownloadUiStore {
  isDrawerOpen: boolean;
  listenerError: string | null;
  setListenerError: (error: string | null) => void;
  openDrawer: () => void;
  closeDrawer: () => void;
  toggleDrawer: () => void;
}

export const useDownloadStore = create<DownloadUiStore>((set) => ({
  isDrawerOpen: false,
  listenerError: null,
  setListenerError: (listenerError) => set({ listenerError }),
  openDrawer: () => set({ isDrawerOpen: true }),
  closeDrawer: () => set({ isDrawerOpen: false }),
  toggleDrawer: () => set((state) => ({ isDrawerOpen: !state.isDrawerOpen })),
}));
