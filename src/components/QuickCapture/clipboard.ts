/**
 * Clipboard helpers for quick capture.
 *
 * Reading the clipboard needs a user gesture in some webviews, so the promise
 * is created inside the key handler and the result arrives later; every read
 * degrades to "no hint" rather than to an error.
 */

/** Reads the clipboard, returning null on any failure (denied, empty, unsupported). */
export async function readClipboardText(): Promise<string | null> {
  try {
    const text = await navigator.clipboard.readText();
    return text.trim() ? text.trim() : null;
  } catch {
    return null;
  }
}

/**
 * The http(s) URL the clipboard holds, or null. Whole-string match only — we
 * never fish a URL out of a paragraph, because the user did not mean to import
 * that.
 */
export function clipboardUrl(text: string | null): URL | null {
  if (!text) return null;
  try {
    const url = new URL(text);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url : null;
  } catch {
    return null;
  }
}
