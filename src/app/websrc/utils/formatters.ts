/**
 * Format a timestamp as a relative time string (e.g., "2h ago", "3d ago")
 *
 * @param timestamp - ISO timestamp string
 * @param fallback - Value to return if timestamp is invalid (default: 'Recently')
 * @returns Formatted relative time string
 */
export function formatRelativeTime(timestamp?: string, fallback = 'Recently'): string {
  if (!timestamp || timestamp === 'Never') return timestamp || fallback;

  try {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;

    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;

    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString();
  } catch {
    return fallback;
  }
}

/**
 * Format bytes to human-readable string (e.g., "1.5 MB", "2.3 GB")
 *
 * @param bytes - Number of bytes
 * @returns Formatted string with appropriate unit
 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${Math.round((bytes / Math.pow(k, i)) * 100) / 100  } ${  sizes[i]}`;
}
