import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, FavoriteDocument } from '@/types';

export const favoritesApi = {
  addFavorite: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('add_favorite', { documentId }),

  removeFavorite: async (documentId: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_favorite', { documentId }),

  getFavorites: async (): Promise<ApiResult<FavoriteDocument[]>> =>
    apiCall<Wire.FavoriteDocument[]>('get_favorites'),

  isFavorite: async (documentId: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('is_favorite', { documentId }),
};
