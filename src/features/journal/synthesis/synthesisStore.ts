import { create } from 'zustand';

import type { SynthesisProgressDto } from '@/lib/bindings';

export type SynthesisStage = SynthesisProgressDto['stage'] | 'saving';
export interface SynthesisJob {
  id: string;
  title: string;
  conversationIds: string[];
  status: 'running' | 'completed' | 'failed';
  stage: SynthesisStage;
  startedAt: number;
  finishedAt?: number;
  entryCount?: number;
  chunkIndex?: number;
  chunkCount?: number;
  noteId?: string;
  noteTitle?: string;
  error?: string;
  retry?: () => Promise<boolean>;
}

interface SynthesisState {
  job: SynthesisJob | null;
  minimized: boolean;
  setMinimized: (minimized: boolean) => void;
  dismiss: () => void;
}

/** Transient activity survives route changes; saved pages remain repository-owned. */
export const useSynthesisStore = create<SynthesisState>((set) => ({
  job: null,
  minimized: false,
  setMinimized: (minimized) => set({ minimized }),
  dismiss: () => set(state => state.job?.status === 'running' ? state : { job: null, minimized: false }),
}));

export const selectSynthesisRunning = (state: SynthesisState) => state.job?.status === 'running';
