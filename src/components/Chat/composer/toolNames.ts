/**
 * The tool names a turn can switch on, and how one reads on screen.
 *
 * These sat inside `ComposerControls` while the gear popover was the only way
 * to reach them. The slash menu and the mode chips flip the same switches, so
 * the names live on their own rather than being imported from the panel that
 * happened to own them first.
 */

export const WEB_TOOL_NAMES = ['web_search', 'fetch_url_content'] as const;
export const WIKI_TOOL_NAMES = ['wiki_search', 'wiki_summary'] as const;

/** `read_inbox` reads as "Read Inbox" wherever a custom tool is named. */
export const formatToolLabel = (name: string): string =>
  name
    .split('_')
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(' ');
