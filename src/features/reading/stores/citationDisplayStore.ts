import { create } from 'zustand';

const STORAGE_KEY = 'reading.citations.visible';

function readVisible(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) !== '0';
  } catch {
    return true;
  }
}

function writeVisible(value: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, value ? '1' : '0');
  } catch {
    // A display preference should never make the reading surface fail.
  }
}

interface CitationDisplayState {
  /** Presentation only. Source bindings and verification reports stay intact. */
  visible: boolean;
  setVisible: (visible: boolean) => void;
  toggle: () => void;
}

export const useCitationDisplayStore = create<CitationDisplayState>((set, get) => ({
  visible: readVisible(),
  setVisible: (visible) => {
    writeVisible(visible);
    set({ visible });
  },
  toggle: () => {
    const visible = !get().visible;
    writeVisible(visible);
    set({ visible });
  },
}));

export const citationDisplayStore = useCitationDisplayStore;
export const CITATION_DISPLAY_STORAGE_KEY = STORAGE_KEY;
