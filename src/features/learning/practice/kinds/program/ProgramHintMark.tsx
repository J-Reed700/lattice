import type * as Wire from '@/lib/bindings';

import { programWork } from './work';

function lineList(mark: Wire.ProgramHintMarkDto): string {
  const where = mark.path ? ` of ${mark.path}` : '';
  return `Look at line${mark.lines.length === 1 ? '' : 's'} ${mark.lines.join(', ')}${where}`;
}

export function ProgramHintMark({ session, hint }: { session: Wire.PracticeSessionDto; hint: Wire.PracticeHintDto }) {
  const mark = programWork(session).hintMarks.find((candidate) => candidate.ordinal === hint.ordinal);
  if (hint.rung !== 'point' || !mark || mark.lines.length === 0) return null;
  return <p className="m-0 mt-1 font-mono text-sm text-danger-fg">{lineList(mark)}</p>;
}
