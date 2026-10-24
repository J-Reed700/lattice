/**
 * Search Result Metadata Types
 *
 * These types define the metadata structure returned by the backend search API.
 * The backend returns snake_case field names that match the database schema.
 */

/**
 * Complete metadata structure for search results
 * Matches the backend's document metadata schema
 * Extends Record<string, unknown> for index signature compatibility
 */
export interface SearchResultMetadata extends Record<string, unknown> {
  /** File system path to the document */
  path: string;
  /** Original filename */
  filename: string;
  /** MIME type or file extension (e.g., 'pdf', 'txt', 'application/pdf') */
  file_type: string | null;
  /** File size in bytes */
  file_size: number;
  /** ISO 8601 timestamp of last file modification */
  updated_at: string;
  /** ISO 8601 timestamp when document was indexed */
  created_at: string;
}

/**
 * Filter options for search queries
 * All fields are optional and will be combined with AND logic
 */
export interface SearchFilter {
  /** Filter by file type (extension or MIME type) */
  fileType?: string;
  /** Filter by file path (partial match) */
  path?: string;
  /** Filter by minimum file size in bytes */
  minSize?: number;
  /** Filter by maximum file size in bytes */
  maxSize?: number;
  /** Filter by documents modified after this date (ISO 8601) */
  modifiedAfter?: string;
  /** Filter by documents modified before this date (ISO 8601) */
  modifiedBefore?: string;
}

/**
 * Type guard to check if metadata has required fields
 */
export function isValidMetadata(metadata: unknown): metadata is SearchResultMetadata {
  if (!metadata || typeof metadata !== 'object') {
    return false;
  }

  // Type guard: after checking typeof === 'object', TypeScript knows metadata is Record<string, unknown>
  // Use 'in' operator for type-safe property access
  return (
    'path' in metadata && typeof metadata.path === 'string' &&
    'filename' in metadata && typeof metadata.filename === 'string' &&
    'file_type' in metadata && (metadata.file_type === null || typeof metadata.file_type === 'string') &&
    'file_size' in metadata && typeof metadata.file_size === 'number' &&
    'updated_at' in metadata && typeof metadata.updated_at === 'string' &&
    'created_at' in metadata && typeof metadata.created_at === 'string'
  );
}

/**
 * Helper to safely extract metadata from unknown data
 * Returns default values if metadata is invalid
 */
export function extractMetadata(data: unknown): SearchResultMetadata {
  if (isValidMetadata(data)) {
    return data;
  }

  // Fallback for invalid metadata
  const now = new Date().toISOString();
  return {
    path: '',
    filename: 'Unknown',
    file_type: null,
    file_size: 0,
    updated_at: now,
    created_at: now,
  };
}

/**
 * Performance metadata for search operations
 */
export interface PerformanceStats {
  count: number;
  min: number;
  max: number;
  avg: number;
  median: number;
  p95: number;
  p99: number;
}

/**
 * Type-safe report structure for performance metrics
 */
export type PerformanceReport = Record<string, PerformanceStats | null>;
