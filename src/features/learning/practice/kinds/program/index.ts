import { CodingEntry } from './CodingEntry';
import { programCopy } from './copy';
import { prepareProgram } from './prepare';
import { ProgramDebrief } from './ProgramDebrief';
import { ProgramHintMark } from './ProgramHintMark';
import { ProgramResults } from './ProgramResults';
import { ProgramTask } from './ProgramTask';
import { ProgramWork } from './ProgramWork';
import { useProgramDraft } from './useProgramDraft';
import type { KindView } from '../../session/kind';

export const programKind: KindView = {
  copy: programCopy,
  prepare: prepareProgram,
  useDraft: useProgramDraft,
  Task: ProgramTask,
  Work: ProgramWork,
  Results: ProgramResults,
  HintMark: ProgramHintMark,
  Debrief: ProgramDebrief,
  Entry: CodingEntry,
};
