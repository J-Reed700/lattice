import { useQuery } from '@tanstack/react-query';
import { create } from 'zustand';

import { isSynthesisLive, synthesesQueryOptions, type JournalSynthesisDto } from './api';

/** A synthesis saved to the Journal; its job is gone, so the panel keeps what it says about it. */
export interface SavedSynthesis {
  jobId: string;
  title: string;
  noteId: string;
  noteTitle: string;
  entryCount: number;
  startedAt: number;
  finishedAt: number;
}

interface SynthesisPanelState {
  minimized: boolean;
  /** Syntheses started in this window: each is saved as soon as it finishes. */
  autoSave: readonly string[];
  /** Saves in flight, by job, and why the last one failed when it did. */
  saving: Readonly<Record<string, { error?: string }>>;
  saved: SavedSynthesis | null;
  setMinimized: (minimized: boolean) => void;
}

/**
 * The progress panel's own state. What a synthesis is doing comes from its
 * job; this only holds what the panel shows around it.
 */
export const useSynthesisPanel = create<SynthesisPanelState>((set) => ({
  minimized: false,
  autoSave: [],
  saving: {},
  saved: null,
  setMinimized: (minimized) => set({ minimized }),
}));

export function setSaving(jobId: string, state: { error?: string } | null): void {
  useSynthesisPanel.setState((panel) => {
    const saving = { ...panel.saving };
    if (state) saving[jobId] = state;
    else delete saving[jobId];
    return { saving };
  });
}

/** The synthesis being written or saved, if any: another waits until it is done. */
export function useActiveSynthesis(): JournalSynthesisDto | null {
  const { data } = useQuery(synthesesQueryOptions());
  const saving = useSynthesisPanel(state => state.saving);
  return data?.find(synthesis => isSynthesisLive(synthesis)
    || (saving[synthesis.job.id] !== undefined && !saving[synthesis.job.id].error)) ?? null;
}
