import { create } from 'zustand';

const EXPANDED_KEY = 'chat.navigation.expanded';

function readExpanded(): boolean {
  try {
    return localStorage.getItem(EXPANDED_KEY) === 'true';
  } catch {
    return false;
  }
}

/** Only the disclosure preference persists; previews stay in conversation data. */
export const useChatNavigationStore = create<{
  expanded: boolean;
  setExpanded: (expanded: boolean) => void;
}>((set) => ({
  expanded: readExpanded(),
  setExpanded: (expanded) => {
    set({ expanded });
    try {
      localStorage.setItem(EXPANDED_KEY, String(expanded));
    } catch {
      // Navigation still works when preference storage is unavailable.
    }
  },
}));
