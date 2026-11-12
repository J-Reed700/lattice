/**
 * Passage references are saved excerpts from documents.
 *
 * The backend slice (`features/references`) and the ReferenceInbox belong to
 * The reading surface creates references and reads the
 * list to mark citations that came from them. Imported directly rather than
 * through the `types` barrel so both tracks can declare it without colliding.
 */
export type {
  CreatePassageReferenceRequestDto,
  PassageReferenceDto,
} from '../../lib/bindings';

export type CreatePassageReferenceRequest = import('../../lib/bindings').CreatePassageReferenceRequestDto;
