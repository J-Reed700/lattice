import { programKind } from './program';
import type { Kinds, KindView } from '../session/kind';

/** Every kind of practice this app can open, by the id the wire carries. */
export const kinds: Kinds = { program: programKind };

/** The kind the Code card starts. */
export const codeKind: KindView = programKind;
