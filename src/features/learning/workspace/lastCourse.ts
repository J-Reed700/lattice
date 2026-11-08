const KEY = 'lattice.studio.lastCourse';

/** The course the learner opened last, if it is still known. */
export function readLastCourse(): string | null {
  try {
    return window.localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function writeLastCourse(programId: string): void {
  try {
    window.localStorage.setItem(KEY, programId);
  } catch {
    // Nothing is kept when the browser will not store it.
  }
}
