/**
 * Tags API Types
 *
 * Type definitions for tagging functionality.
 * These types match the Rust backend DDD structures from commands/tags.rs
 */

// === Request DTOs ===

export interface CreateTagRequest {
  name: string;
  color?: string;
}

export interface UpdateTagRequest {
  tagId: string;
  name?: string;
  color?: string;
}

export interface DeleteTagRequest {
  tagId: string;
}

export interface AddTagToDocumentRequest {
  documentId: string;
  tagId: string;
}

export interface RemoveTagFromDocumentRequest {
  documentId: string;
  tagId: string;
}

export interface GetDocumentTagsRequest {
  documentId: string;
}

export interface GetTagDocumentsRequest {
  tagId: string;
}

export interface SearchTagsRequest {
  query: string;
}

export interface RenameTagRequest {
  tagId: string;
  newName: string;
}

export interface GetTagStatsRequest {
  tagId: string;
}

export interface MergeTagsRequest {
  sourceTagId: string;
  targetTagId: string;
}

export interface ListTagsRequest {
  // Empty for now - may add filters later
}

export interface SetTagColorRequest {
  tagId: string;
  color: string;
}

export interface FindDocumentsByTagRequest {
  tagName: string;
}

// === Response DTOs ===

export interface Tag {
  id: string;
  name: string;
  color?: string;
  documentCount?: number;
  createdAt: string;
  updatedAt: string;
}

export interface TagDto {
  id: string;
  name: string;
  color?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateTagResponse {
  tag: Tag;
  message: string;
}

export interface ListTagsResponse {
  tags: Tag[];
}

export interface RenameTagResponse {
  tag: Tag;
}

export interface SetTagColorResponse {
  tag: Tag;
}

export interface DocumentTagsResponse {
  documentId: string;
  tags: Tag[];
}

export interface SearchTagsResponse {
  tags: Tag[];
  count: number;
}

export interface TagStatsResponse {
  tagId: string;
  name: string;
  documentCount: number;
  recentDocuments: string[];
}

export interface TagStats {
  tagId: string;
  name: string;
  documentCount: number;
  recentDocuments: string[];
}

export interface TaggedDocumentsResponse {
  tagName: string;
  documentIds: string[];
  count: number;
}

// === Legacy Types (deprecated) ===

/**
 * @deprecated Use Tag instead
 */
export interface TagWithCount {
  id: string;
  name: string;
  color: string | null;
  created_at: string;
  document_count: number;
}
