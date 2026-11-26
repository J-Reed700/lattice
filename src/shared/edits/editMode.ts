import { create } from 'zustand';

export const useEditMode = create<{
  on: boolean;
  set: (on: boolean) => void;
  toggle: () => void;
}>((set) => ({
  on: false,
  set: (on) => set({ on }),
  toggle: () => set((state) => ({ on: !state.on })),
}));
