import { newId } from '@/lib/api';

import { ExamplesTable } from './ExamplesTable';
import { ProblemPane } from './ProblemPane';
import { programApi } from './programApi';
import { programWork } from './work';
import type { KindTaskProps } from '../../session/kind';

export function ProgramTask({ programId, session, checking, own, touch, solutionControl }: KindTaskProps) {
  const work = programWork(session);
  const { sessionId } = session;

  const checkExample = async (example: { ordinal: number | null; args: string[]; expected: string }) => {
    touch();
    const ok = await own(() => programApi.checkExample({ operationId: newId(), programId, sessionId, ...example }));
    if (!ok) throw new Error('The example could not be checked.');
  };

  const removeExample = async (ordinal: number) => {
    await own(() => programApi.removeExample({ operationId: newId(), programId, sessionId, ordinal }));
  };

  return (
    <>
      <ProblemPane problem={work.problem} workedSolution={work.workedSolution} />
      {solutionControl}
      <ExamplesTable
        signature={work.problem.signature}
        examples={work.examples}
        coverage={session.coverage}
        busy={checking}
        onCheck={checkExample}
        onRemove={removeExample}
      />
    </>
  );
}
