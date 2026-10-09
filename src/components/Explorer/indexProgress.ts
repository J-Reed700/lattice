/**
 * How a folder index's progress is put into words: the scope bar's pill, the
 * chat column's line and the folders list all say it the same way.
 */

/** `1,379`. */
export const count = (value: number) => value.toLocaleString('en-US');

/** Whole percent of passages embedded; never 100 before the last one is in. */
export function indexPercent(embedded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.floor((Math.min(embedded, total) / total) * 100);
}

/** Time left, for people: "<1 min", "~3 min", "~1 h 10 min". */
export function formatEta(seconds: number): string {
  if (seconds < 60) return '<1 min';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `~${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `~${hours} h ${rest} min` : `~${hours} h`;
}

/** `12 passages/s`, with a decimal while it is small. */
export function formatRate(perSecond: number): string {
  const shown = perSecond < 10 ? perSecond.toFixed(1) : Math.round(perSecond).toString();
  return `${shown} passages/s`;
}

/** `~/Code/lattice` for a folder under the home folder; the path as is otherwise. */
export function tildePath(path: string, home: string | null | undefined): string {
  if (!home) return path;
  if (path === home) return '~';
  return path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}
