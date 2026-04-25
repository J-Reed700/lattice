import { useCallback, useEffect, useState } from 'react';

export interface RecentItem {
  id: string;
  label: string;
  timestamp: number;
}

export interface SpotlightState {
  isOpen: boolean;
  recentSearches: RecentItem[];
  recentDocuments: RecentItem[];
}

const MAX_RECENT_ITEMS = 5;
const STORAGE_KEY = 'vault-command-palette';

/**
 * useSpotlight
 *
 * Global open/close state for the unified Spotlight surface, plus a small
 * localStorage-backed list of recent searches and recent documents.
 *
 * Binds Cmd/Ctrl+K and Cmd/Ctrl+P as the global open shortcut, and Escape
 * to close. The actual result rendering and API fan-out lives inside
 * the Spotlight component.
 *
 * Replaces the former useCommandPalette hook — same storage key, same
 * recents contract, but without the now-unused searchMode toggle.
 */
export function useSpotlight() {
  const [state, setState] = useState<SpotlightState>({
    isOpen: false,
    recentSearches: [],
    recentDocuments: [],
  });

  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const data = JSON.parse(stored);
        setState((prev) => ({
          ...prev,
          recentSearches: Array.isArray(data?.recentSearches) ? data.recentSearches : [],
          recentDocuments: Array.isArray(data?.recentDocuments) ? data.recentDocuments : [],
        }));
      }
    } catch (error) {
      console.error('Failed to load spotlight state:', error);
    }
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify({
          recentSearches: state.recentSearches,
          recentDocuments: state.recentDocuments,
        })
      );
    } catch (error) {
      console.error('Failed to save spotlight state:', error);
    }
  }, [state.recentSearches, state.recentDocuments]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const isMod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();

      if (isMod && (key === 'k' || key === 'p')) {
        event.preventDefault();
        setState((prev) => ({ ...prev, isOpen: !prev.isOpen }));
        return;
      }

      if (event.key === 'Escape') {
        setState((prev) => (prev.isOpen ? { ...prev, isOpen: false } : prev));
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const open = useCallback(() => {
    setState((prev) => ({ ...prev, isOpen: true }));
  }, []);

  const close = useCallback(() => {
    setState((prev) => ({ ...prev, isOpen: false }));
  }, []);

  const toggle = useCallback(() => {
    setState((prev) => ({ ...prev, isOpen: !prev.isOpen }));
  }, []);

  const addRecentSearch = useCallback((search: string) => {
    const trimmed = search.trim();
    if (!trimmed) return;
    setState((prev) => {
      const newItem: RecentItem = {
        id: `search-${Date.now()}`,
        label: trimmed,
        timestamp: Date.now(),
      };
      const filtered = prev.recentSearches.filter((item) => item.label !== trimmed);
      return { ...prev, recentSearches: [newItem, ...filtered].slice(0, MAX_RECENT_ITEMS) };
    });
  }, []);

  const addRecentDocument = useCallback((docId: string, docName: string) => {
    setState((prev) => {
      const newItem: RecentItem = {
        id: docId,
        label: docName,
        timestamp: Date.now(),
      };
      const filtered = prev.recentDocuments.filter((item) => item.id !== docId);
      return { ...prev, recentDocuments: [newItem, ...filtered].slice(0, MAX_RECENT_ITEMS) };
    });
  }, []);

  const clearRecentSearches = useCallback(() => {
    setState((prev) => ({ ...prev, recentSearches: [] }));
  }, []);

  const clearRecentDocuments = useCallback(() => {
    setState((prev) => ({ ...prev, recentDocuments: [] }));
  }, []);

  return {
    isOpen: state.isOpen,
    recentSearches: state.recentSearches,
    recentDocuments: state.recentDocuments,
    open,
    close,
    toggle,
    addRecentSearch,
    addRecentDocument,
    clearRecentSearches,
    clearRecentDocuments,
  };
}
