import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, FavoriteDocument } from '@/types';

export const favoritesApi = {
  /**
   * Marks a document as favorite.
   * Adds document to favorites list for quick access.
   *
   * @param documentId - Internal document identifier
   * @returns Void on success
   */
  addFavorite: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('add_favorite', { documentId }),

  /**
   * Removes a document from favorites.
   * Unfavorites the document but doesn't delete it.
   *
   * @param documentId - Internal document identifier
   * @returns Void on success
   */
  removeFavorite: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_favorite', { documentId }),

  /**
   * Retrieves all favorited documents.
   * Returns documents in the order they were favorited.
   *
   * @returns Array of favorite document objects
   */
  getFavorites: async (): Promise<ApiResult<FavoriteDocument[]>> =>
    apiCall<Wire.FavoriteDocument[]>('get_favorites'),

  /**
   * Checks if a document is marked as favorite.
   *
   * @param documentId - Internal document identifier
   * @returns True if document is favorited, false otherwise
   */
  isFavorite: async (documentId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_favorite', { documentId }),
};
