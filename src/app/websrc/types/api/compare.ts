/** Comparison tables across documents. Contract: GROUND-RULES §4.10. */
export interface CompareCitationDto {
  chunkId: string;
  excerpt: string;
}

export interface CompareCellDto {
  /** null renders as "not stated" — never as a guess. */
  value: string | null;
  citation: CompareCitationDto | null;
}

export interface CompareRowDto {
  documentId: string;
  title: string;
  filePath: string;
  /** Same length and order as CompareTableDto.columns. */
  cells: CompareCellDto[];
  /** Set when this document could not be processed. Cells are all null. */
  error: string | null;
}

export interface CompareTableDto {
  columns: string[];
  rows: CompareRowDto[];
  modelName: string;
  generatedAt: string;
}

export interface CompareDocumentsRequest {
  documentIds: string[];
  columns: string[];
}
