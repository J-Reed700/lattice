/**
 * Unified Formatting Helpers
 *
 * Canonical formatters for bytes, relative time, ETA, time left, and percentages.
 */

export interface RelativeTimeOptions {
  compact?: boolean;
}

/**
 * Format a timestamp as a relative time string (e.g., "2h ago", "3d ago", "1w ago")
 *
 * @param timestamp - ISO timestamp string or Date
 * @param fallback - Value to return if timestamp is invalid (default: 'Recently')
 * @param options - Options or boolean for compact style
 * @returns Formatted relative time string
 */
export function formatRelativeTime(
  timestamp?: string | Date | null,
  fallback = 'Recently',
  options?: RelativeTimeOptions | boolean
): string {
  if (!timestamp || timestamp === 'Never') return (typeof timestamp === 'string' && timestamp) || fallback;

  try {
    const date = typeof timestamp === 'string' ? new Date(timestamp) : timestamp;
    if (!Number.isFinite(date.getTime())) return fallback;
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;

    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;

    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;

    const isCompact = typeof options === 'boolean' ? options : Boolean(options?.compact);
    if (isCompact) {
      if (diffDays < 30) return `${Math.floor(diffDays / 7)}w ago`;
      return `${Math.floor(diffDays / 30)}mo ago`;
    }

    return date.toLocaleDateString();
  } catch {
    return fallback;
  }
}

/**
 * Relative date formatter returning null if timestamp is absent/null.
 */
export function formatRelativeDate(timestamp?: string | Date | null): string | null {
  if (!timestamp) return null;
  return formatRelativeTime(timestamp);
}

/**
 * Format bytes to human-readable string (e.g., "1.5 MB", "2.3 GB")
 *
 * @param bytes - Number of bytes
 * @param options - Formatting options, e.g. return empty string for non-positive values
 * @returns Formatted string with appropriate unit
 */
export interface FormatBytesOptions {
  emptyForNonPositive?: boolean;
  decimals?: number;
  fixedDecimals?: boolean;
}

export function formatBytes(
  bytes: number,
  options?: FormatBytesOptions
): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return options?.emptyForNonPositive ? '' : '0 B';
  }
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.min(sizes.length - 1, Math.max(0, Math.floor(Math.log(bytes) / Math.log(k))));
  const value = bytes / Math.pow(k, i);
  if (options?.fixedDecimals && i > 0) {
    const dec = options.decimals ?? 1;
    return `${value.toFixed(dec)} ${sizes[i]}`;
  }
  if (options?.decimals !== undefined) {
    const factor = Math.pow(10, options.decimals);
    return `${Math.round(value * factor) / factor} ${sizes[i]}`;
  }
  return `${Math.round(value * 100) / 100} ${sizes[i]}`;
}

export const formatFileSize = formatBytes;

export interface FormatTimeLeftOptions {
  suffix?: boolean;
}

/**
 * Format time left in human terms: "less than a minute", "about 11 min", "about 1 h 10 min".
 *
 * @param seconds - Number of seconds remaining
 * @param options - Options (or boolean) indicating whether to include " left" suffix
 */
export function formatTimeLeft(
  seconds: number,
  options?: FormatTimeLeftOptions | boolean
): string {
  const withSuffix = typeof options === 'boolean' ? options : Boolean(options?.suffix);
  if (!Number.isFinite(seconds) || seconds < 60) return 'less than a minute';
  const minutes = Math.round(seconds / 60);
  const suffix = withSuffix ? ' left' : '';
  if (minutes < 60) return `about ${minutes} min${suffix}`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `about ${hours} h${suffix}` : `about ${hours} h ${rest} min${suffix}`;
}

export interface FormatEtaOptions {
  style?: 'approximate' | 'compact';
}

/**
 * Format ETA for operations.
 * - approximate: "<1 min", "~3 min", "~1 h", "~1 h 10 min"
 * - compact: "0s", "59s", "1m 0s", "59m 59s", "1h 0m", "1h 1m", or "—" for invalid/negative
 */
export function formatEta(
  seconds: number | null | undefined,
  options?: FormatEtaOptions | 'approximate' | 'compact'
): string {
  const style = typeof options === 'string' ? options : options?.style ?? 'approximate';
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) {
    return style === 'compact' ? '—' : '<1 min';
  }
  if (style === 'compact') {
    const total = Math.round(seconds);
    if (total < 60) return `${total}s`;
    const minutes = Math.floor(total / 60);
    if (minutes < 60) return `${minutes}m ${total % 60}s`;
    const hours = Math.floor(minutes / 60);
    return `${hours}h ${minutes % 60}m`;
  }
  if (seconds < 60) return '<1 min';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `~${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `~${hours} h ${rest} min` : `~${hours} h`;
}

/** Whole percent embedded; never 100 before the last one is in. Clamped 0-100 with Math.floor. */
export function indexPercent(embedded: number, total: number): number {
  if (!Number.isFinite(embedded) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.floor((Math.min(Math.max(0, embedded), total) / total) * 100);
}

/** Whole percent done, 0–100 with Math.round. Accepts object with done/total or two numbers. */
export function progressPercent(
  progressOrDone: { done: number; total: number; [key: string]: any } | number,
  total?: number
): number {
  const done = typeof progressOrDone === 'number' ? progressOrDone : progressOrDone.done;
  const tot = typeof progressOrDone === 'number' ? (total ?? 0) : progressOrDone.total;
  if (!Number.isFinite(done) || !Number.isFinite(tot) || tot <= 0) return 0;
  return Math.min(100, Math.round((Math.max(0, done) / tot) * 100));
}
