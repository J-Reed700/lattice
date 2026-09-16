const DEFAULT_TITLE_PREFIX = 'Untitled';

/**
 * Generates a human-friendly default conversation title.
 * Example: "Untitled Feb 26, 2:15 PM"
 */
export const createDefaultConversationTitle = (now: Date = new Date()): string => {
  const dateLabel = now.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
  });
  const timeLabel = now.toLocaleTimeString(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  return `${DEFAULT_TITLE_PREFIX} ${dateLabel}, ${timeLabel}`;
};
