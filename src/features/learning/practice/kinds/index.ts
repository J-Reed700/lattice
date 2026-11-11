import { programKind } from './program';

import type { Kinds } from '../session/kind';

export { CodingEntry, type CodingBuildChoice } from './program';

/** The id the wire carries for Program. */
export const PROGRAM_KIND_ID = 'program';

/** Every kind of practice this app can open, by the id the wire carries. */
export const kinds: Kinds = { [PROGRAM_KIND_ID]: programKind };
