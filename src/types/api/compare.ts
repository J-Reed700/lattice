/** Comparison tables across documents. */
export type CompareCitationDto = import('../../lib/bindings').CompareCitationDto;

export type CompareCellDto = import('../../lib/bindings').CompareCellDto;

export type CompareRowDto = import('../../lib/bindings').CompareRowDto;

export type CompareTableDto = import('../../lib/bindings').CompareTableDto;

export interface CompareDocumentsRequest {
  documentIds: string[];
  columns: string[];
}
