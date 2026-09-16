/**
 * Tag System Type Definitions
 *
 * TypeScript types matching Rust backend DTOs for the tag management system.
 * These types define the contract between frontend and backend for all tag operations.
 */

// ============================================================================
// Request Types
// ============================================================================

/**
 * Request to create a new tag
 */
export interface CreateTagRequest {
  /** Tag name (1-100 characters, alphanumeric with spaces/hyphens/underscores) */
  name: string;
  /** Optional hex color code (e.g., "#FF5733") */
  color?: string;
}

/**
 * Request to rename an existing tag
 */
export interface RenameTagRequest {
  /** UUID of the tag to rename */
  tag_id: string;
  /** New tag name */
  new_name: string;
}

/**
 * Request to change a tag's color
 */
export interface ChangeTagColorRequest {
  /** UUID of the tag */
  tag_id: string;
  /** New hex color code (e.g., "#FF5733") */
  new_color: string;
}

/**
 * Request to delete a tag
 */
export interface DeleteTagRequest {
  /** UUID of the tag to delete */
  tag_id: string;
}

/**
 * Request to add a tag to a document
 */
export interface AddTagToDocumentRequest {
  /** UUID of the document */
  document_id: string;
  /** UUID of the tag to add */
  tag_id: string;
}

/**
 * Request to remove a tag from a document
 */
export interface RemoveTagFromDocumentRequest {
  /** UUID of the document */
  document_id: string;
  /** UUID of the tag to remove */
  tag_id: string;
}

/**
 * Request to get all tags for a document
 */
export interface GetDocumentTagsRequest {
  /** UUID of the document */
  document_id: string;
}

/**
 * Request to get all documents with a specific tag
 */
export interface GetDocumentsByTagRequest {
  /** UUID of the tag */
  tag_id: string;
}

/**
 * Request to list all tags with optional filtering
 */
export interface ListTagsRequest {
  /** Optional search query to filter tags by name */
  search_query?: string;
}

/**
 * Request to get a specific tag by ID
 */
export interface GetTagRequest {
  /** UUID of the tag */
  tag_id: string;
}

/**
 * Request to get tag statistics (document count)
 */
export interface GetTagStatsRequest {
  /** UUID of the tag */
  tag_id: string;
}

// ============================================================================
// Response Types
// ============================================================================

/**
 * Tag data transfer object
 */
export interface TagDto {
  /** UUID of the tag */
  id: string;
  /** Tag name */
  name: string;
  /** Hex color code (e.g., "#FF5733") */
  color: string;
  /** ISO 8601 timestamp of creation */
  created_at: string;
  /** ISO 8601 timestamp of last update */
  updated_at: string;
}

/**
 * Response after creating a tag
 */
export interface CreateTagResponse {
  /** The newly created tag */
  tag: TagDto;
}

/**
 * Response after renaming a tag
 */
export interface RenameTagResponse {
  /** The updated tag */
  tag: TagDto;
}

/**
 * Response after changing a tag's color
 */
export interface ChangeTagColorResponse {
  /** The updated tag */
  tag: TagDto;
}

/**
 * Response with a list of tags
 */
export interface ListTagsResponse {
  /** Array of tags */
  tags: TagDto[];
}

/**
 * Response with a list of document IDs
 */
export interface GetDocumentsByTagResponse {
  /** Array of document UUIDs that have the specified tag */
  document_ids: string[];
}

/**
 * Response with tag statistics
 */
export interface GetTagStatsResponse {
  /** UUID of the tag */
  tag_id: string;
  /** Tag name */
  tag_name: string;
  /** Number of documents with this tag */
  document_count: number;
}
