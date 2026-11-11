import type * as Wire from '@/lib/bindings';

/** What the wire's opaque `work` holds for the program kind. */
export function programWork(session: Wire.PracticeSessionDto): Wire.ProgramWorkDto {
  return session.work as unknown as Wire.ProgramWorkDto;
}

/** What the debrief's opaque `work` holds for the program kind. */
export function programDebriefWork(debrief: Wire.PracticeDebriefDto): Wire.ProgramDebriefWorkDto {
  return debrief.work as unknown as Wire.ProgramDebriefWorkDto;
}
