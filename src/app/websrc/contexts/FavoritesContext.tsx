/**
 * Favorites Context
 *
 * Simplified state management for favorites and recent documents.
 * Converted from Zustand store to React Context for simplicity.
 *
 * Why Context instead of Zustand:
 * - Only 3 components use this state
 * - Simple state structure (Set + arrays)
 * - No performance optimization needed
 * - Async operations work fine in Context
 *
 * Usage:
 *   Wrap app with <FavoritesProvider>
 *   Use useFavorites() hook in components
 */

import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react';

import VaultAPI from '../lib/api';
import { createLogger } from '../utils/logger';

const logger = createLogger('FavoritesContext');

// Types
export interface FavoriteDocument {
  id: string;
  document_id: string;
  document_name: string;
  document_path: string;
  file_type: string | null;
  added_at: string;
}

export interface RecentDocument {
  id: string;
  document_id: string;
  document_name: string;
  document_path: string;
  file_type: string | null;
  last_accessed_at: string;
  access_count: number;
}

// Context Type
interface FavoritesContextType {
  // State
  favorites: Set<string>;
  favoriteDocuments: FavoriteDocument[];
  recentDocs: RecentDocument[];
  isLoading: boolean;
  error: string | null;

  // Favorites Actions
  addFavorite: (_docId: string) => Promise<void>;
  removeFavorite: (_docId: string) => Promise<void>;
  isFavorite: (_docId: string) => boolean;
  loadFavorites: () => Promise<void>;

  // Recent Documents Actions
  trackDocumentAccess: (_docId: string) => Promise<void>;
  loadRecentDocs: (_limit?: number) => Promise<void>;
  clearRecentDocs: () => Promise<void>;
}

const FavoritesContext = createContext<FavoritesContextType | null>(null);

// Provider Props
interface FavoritesProviderProps {
  children: ReactNode;
}

export function FavoritesProvider({ children }: FavoritesProviderProps) {
  // State
  const [favorites, setFavorites] = useState<Set<string>>(new Set());
  const [favoriteDocuments, setFavoriteDocuments] = useState<FavoriteDocument[]>([]);
  const [recentDocs, setRecentDocs] = useState<RecentDocument[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load all favorites from backend
  const loadFavorites = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    const result = await VaultAPI.getFavorites();

    if (result.ok) {
      setFavoriteDocuments(result.data);
      setFavorites(new Set(result.data.map(doc => doc.document_id)));
      setError(null);
    } else {
      setError(result.error);
      logger.error('Failed to load favorites', { action: 'loadFavorites', error: result.error });
    }

    setIsLoading(false);
  }, []);

  // Add favorite with optimistic update
  const addFavorite = useCallback(async (docId: string) => {
    // Save previous state for rollback
    const previousFavorites = new Set(favorites);

    // Optimistic update
    setFavorites(new Set([...favorites, docId]));
    setError(null);

    const result = await VaultAPI.addFavorite(docId);

    if (result.ok) {
      // Reload to get full document metadata
      await loadFavorites();
    } else {
      // Rollback on error
      setFavorites(previousFavorites);
      setError(result.error);
      logger.error('Failed to add favorite', { action: 'addFavorite', documentId: docId, error: result.error });
      throw new Error(result.error);
    }
  }, [favorites, loadFavorites]);

  // Remove favorite with optimistic update
  const removeFavorite = useCallback(async (docId: string) => {
    // Save previous state for rollback
    const previousFavorites = new Set(favorites);
    const previousDocuments = [...favoriteDocuments];

    // Optimistic update
    const newFavorites = new Set(favorites);
    newFavorites.delete(docId);
    setFavorites(newFavorites);
    setFavoriteDocuments(favoriteDocuments.filter(doc => doc.document_id !== docId));
    setError(null);

    const result = await VaultAPI.removeFavorite(docId);

    if (!result.ok) {
      // Rollback on error
      setFavorites(previousFavorites);
      setFavoriteDocuments(previousDocuments);
      setError(result.error);
      logger.error('Failed to remove favorite', { action: 'removeFavorite', documentId: docId, error: result.error });
      throw new Error(result.error);
    }
  }, [favorites, favoriteDocuments]);

  // Check if document is favorited
  const isFavorite = useCallback((docId: string) => favorites.has(docId), [favorites]);

  // Load recent documents
  const loadRecentDocs = useCallback(async (limit: number = 10) => {
    setIsLoading(true);
    setError(null);

    const result = await VaultAPI.listAllDocuments(10000);

    if (result.ok) {
      const docs: RecentDocument[] = result.data
        .sort((a, b) => new Date(b.indexedAt).getTime() - new Date(a.indexedAt).getTime())
        .slice(0, limit)
        .map(doc => ({
          id: doc.id,
          document_id: doc.id,
          document_name: doc.fileName,
          document_path: doc.filePath,
          file_type: doc.fileType,
          last_accessed_at: doc.indexedAt,
          access_count: 0,
          // Additional properties used by Dashboard components
          fileName: doc.fileName,
          filePath: doc.filePath,
          indexedAt: doc.indexedAt,
          modifiedAt: doc.modifiedAt,
          sizeBytes: 0, // Not available in DocumentMetadata
        }));

      setRecentDocs(docs);
      setError(null);
    } else {
      setError(result.error);
      logger.error('Failed to load recent documents', { action: 'loadRecentDocs', limit, error: result.error });
    }

    setIsLoading(false);
  }, []);

  // Track document access
  const trackDocumentAccess = useCallback(async (docId: string) => {
    const result = await VaultAPI.trackDocumentAccess(docId);

    if (result.ok) {
      // Silently reload recent docs (don't block UI)
      loadRecentDocs(10).catch(err => logger.error('Failed to reload recent docs', { action: 'trackDocumentAccess' }, err instanceof Error ? err : undefined));
    } else {
      // Don't throw - tracking is non-critical
      logger.error('Failed to track document access', { action: 'trackDocumentAccess', documentId: docId, error: result.error });
    }
  }, [loadRecentDocs]);

  // Clear all recent documents
  const clearRecentDocs = useCallback(async () => {
    const result = await VaultAPI.clearRecentDocuments();

    if (result.ok) {
      setRecentDocs([]);
      setError(null);
    } else {
      setError(result.error);
      logger.error('Failed to clear recent documents', { action: 'clearRecentDocs', error: result.error });
      throw new Error(result.error);
    }
  }, []);

  // Load favorites on mount
  useEffect(() => {
    loadFavorites();
    loadRecentDocs(10);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const value: FavoritesContextType = {
    // State
    favorites,
    favoriteDocuments,
    recentDocs,
    isLoading,
    error,

    // Actions
    addFavorite,
    removeFavorite,
    isFavorite,
    loadFavorites,
    trackDocumentAccess,
    loadRecentDocs,
    clearRecentDocs,
  };

  return (
    <FavoritesContext.Provider value={value}>
      {children}
    </FavoritesContext.Provider>
  );
}

/**
 * Hook to access Favorites context
 * Must be used within FavoritesProvider
 */
export function useFavorites() {
  const context = useContext(FavoritesContext);
  if (!context) {
    throw new Error('useFavorites must be used within FavoritesProvider');
  }
  return context;
}
