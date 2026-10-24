export interface SearchResultMetadata extends Record<string, unknown> {
  path: string;
  filename: string;
  file_type: string | null;
  file_size: number;
  updated_at: string;
  created_at: string;
}

export interface SearchFilter {
  fileType?: string;
  path?: string;
  minSize?: number;
  maxSize?: number;
  modifiedAfter?: string;
  modifiedBefore?: string;
}

export function isValidMetadata(metadata: unknown): metadata is SearchResultMetadata {
  if (!metadata || typeof metadata !== 'object') {
    return false;
  }

  return (
    'path' in metadata && typeof metadata.path === 'string' &&
    'filename' in metadata && typeof metadata.filename === 'string' &&
    'file_type' in metadata && (metadata.file_type === null || typeof metadata.file_type === 'string') &&
    'file_size' in metadata && typeof metadata.file_size === 'number' &&
    'updated_at' in metadata && typeof metadata.updated_at === 'string' &&
    'created_at' in metadata && typeof metadata.created_at === 'string'
  );
}

export function extractMetadata(data: unknown): SearchResultMetadata {
  if (isValidMetadata(data)) {
    return data;
  }

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

export interface PerformanceStats {
  count: number;
  min: number;
  max: number;
  avg: number;
  median: number;
  p95: number;
  p99: number;
}

export type PerformanceReport = Record<string, PerformanceStats | null>;
