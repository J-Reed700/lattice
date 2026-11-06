import { open as openInShell } from '@tauri-apps/plugin-shell';

/**
 * Open a web address in the person's browser.
 *
 * Only `http` and `https`. Addresses reach this from search results and model
 * tool calls, and handing the shell a `file:` or a custom-scheme link because a
 * web page listed one is not something a click on "a source" should ever do.
 *
 * Resolves `false` when nothing could be opened, so a caller can say so.
 */
export async function openExternalUrl(url: string): Promise<boolean> {
  return (await whyExternalUrlDidNotOpen(url)) === null;
}

/**
 * Open a web address as `openExternalUrl` does, and say why it could not be
 * opened: `null` when it opened, otherwise the reason, for the person to read.
 */
export async function whyExternalUrlDidNotOpen(url: string): Promise<string | null> {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return 'It is not a web address.';
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return 'Only http and https addresses open in the browser.';
  try {
    await openInShell(parsed.href);
    return null;
  } catch (error) {
    // Outside the desktop shell (a browser preview) there is no plugin.
    if (window.open(parsed.href, '_blank', 'noopener,noreferrer') !== null) return null;
    return error instanceof Error ? error.message : String(error);
  }
}
