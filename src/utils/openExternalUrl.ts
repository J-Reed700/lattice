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
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false;
  try {
    await openInShell(parsed.href);
    return true;
  } catch {
    // Outside the desktop shell (a browser preview) there is no plugin.
    return window.open(parsed.href, '_blank', 'noopener,noreferrer') !== null;
  }
}
