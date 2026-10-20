import type { ActivityEntryDto, ActivityProgress } from '@/lib/bindings';

/**
 * Time left as people say it: "less than a minute", "about 12 min left",
 * "about 1 h 20 min left". Only ever given a time the backend measured.
 */
export function formatTimeLeft(seconds: number): string {
  if (seconds < 60) return 'less than a minute';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `about ${minutes} min left`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `about ${hours} h left` : `about ${hours} h ${rest} min left`;
}

/** When a job put off by an outage tries again: "Tries again in about 3 min". */
export function formatRetry(retryAt: number, now: number): string {
  const seconds = Math.max(0, (retryAt - now) / 1000);
  if (seconds < 60) return 'Tries again in less than a minute';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `Tries again in about ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `Tries again in about ${hours} h` : `Tries again in about ${hours} h ${rest} min`;
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${unit === 0 ? value : value.toFixed(1)} ${BYTE_UNITS[unit]}`;
}

/** "120 of 900 passages", "1.2 GB of 4.5 GB". */
export function formatProgress(progress: ActivityProgress): string {
  if (progress.unit === 'bytes') return `${formatBytes(progress.done)} of ${formatBytes(progress.total)}`;
  return `${progress.done.toLocaleString()} of ${progress.total.toLocaleString()} ${progress.unit}`;
}

/** Whole percent done, 0–100. */
export function progressPercent(progress: ActivityProgress): number {
  if (progress.total <= 0) return 0;
  return Math.min(100, Math.round((progress.done / progress.total) * 100));
}

const STATE_WORDS: Record<ActivityEntryDto['state'], string> = {
  running: 'Running',
  paused: 'Paused',
  waiting: 'Waiting',
};

/**
 * The line under an item's title: what it is doing, or plainly why it waits
 * or holds, and when a job put off by an outage tries again.
 */
export function stateLine(item: ActivityEntryDto, now: number): string {
  const said = item.detail ?? STATE_WORDS[item.state];
  if (item.state === 'waiting' && item.retryAt !== null && item.retryAt > now) {
    return `${said}. ${formatRetry(item.retryAt, now)}`;
  }
  return said;
}

/** The trigger's label: "Activity: 3 running", "Activity: nothing running". */
export function activityLabel(running: number, total: number): string {
  if (running > 0) return `Activity: ${running} running`;
  if (total > 0) return `Activity: ${total} waiting or paused`;
  return 'Activity: nothing running';
}

/** "3 running, 1 paused, 2 waiting", or "Nothing is running." */
export function activitySummary(items: readonly ActivityEntryDto[]): string {
  if (items.length === 0) return 'Nothing is running.';
  const count = (state: ActivityEntryDto['state']) => items.filter((item) => item.state === state).length;
  const parts = [
    [count('running'), 'running'],
    [count('paused'), 'paused'],
    [count('waiting'), 'waiting'],
  ] as const;
  return parts
    .filter(([n]) => n > 0)
    .map(([n, word]) => `${n} ${word}`)
    .join(', ');
}
