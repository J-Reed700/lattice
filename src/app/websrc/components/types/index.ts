export interface SearchResultMetadata {
  filename?: string;
  path?: string;
  file_type?: string | null;
  size_bytes?: number;
  modified_at?: string;
  indexed_at?: string;
  tags?: string[];
  [key: string]: unknown;
}
