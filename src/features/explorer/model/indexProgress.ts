/**
 * How a folder index's progress is put into words: the scope bar's pill, the
 * chat column's line and the folders list all say it the same way.
 */

import { formatEta, indexPercent } from '@/utils/formatters';

export { formatEta, indexPercent };

/** `1,379`. */
export const count = (value: number) => value.toLocaleString('en-US');

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
