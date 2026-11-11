import type * as Wire from '@/lib/bindings';

import { programApi } from './programApi';

/** The revision of the activity the learner's code is saved under; 0 for a finished session, which saves nothing. */
export function prepareProgram(programId: string, session: Wire.PracticeSessionDto): Promise<number> {
  return session.finished ? Promise.resolve(0) : programApi.activityRevision(programId, session.activityId);
}
