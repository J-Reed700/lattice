import { create } from 'zustand';

interface QuizStopState {
  stoppedJobsByForm: Record<string, string>;
  markStopped: (formId: string, jobId: string) => void;
  clearStopped: (formId: string) => void;
}

const initialState = () => ({ stoppedJobsByForm: {} as Record<string, string> });

/** Tracks quiz forms whose grading job was explicitly stopped by the learner across route/tab switches. */
export const useQuizStopStore = create<QuizStopState>((set) => ({
  ...initialState(),
  markStopped: (formId, jobId) => set((state) => ({
    stoppedJobsByForm: { ...state.stoppedJobsByForm, [formId]: jobId },
  })),
  clearStopped: (formId) => set((state) => {
    if (!(formId in state.stoppedJobsByForm)) return state;
    const next = { ...state.stoppedJobsByForm };
    delete next[formId];
    return { stoppedJobsByForm: next };
  }),
}));

export function resetQuizStopStore(): void {
  useQuizStopStore.setState(initialState());
}
