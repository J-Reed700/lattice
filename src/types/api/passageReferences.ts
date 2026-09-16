/**
 * Passage references are saved excerpts from documents.
 *
 * The backend slice (`features/references`) and the ReferenceInbox belong to
 * The reading surface creates references and reads the
 * list to mark citations that came from them. Imported directly rather than
 * through the `types` barrel so both tracks can declare it without colliding.
 */
export interface PassageReferenceDto {
  id: string;
  documentId: string;
  chunkId: string | null;
  filePath: string;
  fileName: string;
  /** Human-readable position: "p. 12", "12:40", "§ Methods". */
  locator: string | null;
  text: string;
  title: string | null;
  note: string | null;
  createdAt: string;
}

export interface CreatePassageReferenceRequest {
  documentId: string;
  chunkId?: string;
  filePath: string;
  fileName: string;
  locator?: string;
  text: string;
  title?: string;
  note?: string;
}
