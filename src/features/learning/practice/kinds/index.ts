import { programKind } from './program';

import type { Kinds } from '../session/kind';

export { CodingEntry, type CodingBuildChoice } from './program';

/** Every kind of practice this app can open, by the id the wire carries. */
export const kinds: Kinds = { program: programKind };
