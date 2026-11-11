import { useMemo, useRef } from 'react';

import type * as Wire from '@/lib/bindings';

import { programWork } from './work';
import { useLearningLabDraft } from '../../practical/useLearningLabDraft';
import type { KindDraft } from '../../session/kind';

/** What Program's components read from its draft. */
export interface ProgramDraftState {
  files: Wire.LearningLabFile[];
  setFileContent: (path: string, content: string) => void;
  /** The files as they stand now, for a call that outlives a render. */
  currentFiles: () => Wire.LearningLabFile[];
}

export function programDraftState(draft: KindDraft): ProgramDraftState {
  return draft.state as ProgramDraftState;
}

/** The activity revision `prepare` read; 0 for a session that is finished. */
function revisionOf(prepared: unknown): number {
  return typeof prepared === 'number' ? prepared : 0;
}

export function useProgramDraft({
  programId,
  session,
  prepared,
}: {
  programId: string;
  session: Wire.PracticeSessionDto;
  prepared: unknown;
}): KindDraft {
  const starterFiles = useMemo(() => programWork(session).draft, [session]);
  const lab = useLearningLabDraft({
    programId,
    activityId: session.activityId,
    activityRevision: revisionOf(prepared),
    starterFiles,
  });
  const files = lab.files;
  const filesRef = useRef(files);
  filesRef.current = files;

  const failed = lab.saveState === 'error' || lab.saveState === 'conflict';
  return {
    piece: () => filesRef.current,
    signature: files.map((file) => `${file.path}\u0000${file.content}`).join('\u0001'),
    flush: async () => {
      await lab.flush();
    },
    failure: failed
      ? {
          message: lab.error ?? 'Your code could not be saved.',
          retry: () => void (lab.saveState === 'conflict' ? lab.keepLocalEdits() : lab.flush()),
        }
      : null,
    state: { files, setFileContent: lab.setFileContent, currentFiles: () => filesRef.current },
  };
}
