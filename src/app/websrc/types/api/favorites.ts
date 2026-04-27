/**
 * Favorites API Types
 *
 * Type definitions for document favorites/bookmarks.
 * These types match the Rust backend structures from commands/favorites.rs
 */

export interface FavoriteDocument {
  id: string;
  document_id: string;
  document_name: string;
  document_path: string;
  file_type: string | null;
  added_at: string;
}
