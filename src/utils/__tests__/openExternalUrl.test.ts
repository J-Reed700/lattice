import { beforeEach, describe, expect, it, vi } from 'vitest';

import { openExternalUrl } from '../openExternalUrl';

const openInShell = vi.hoisted(() => vi.fn(async () => undefined));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: openInShell }));

describe('openExternalUrl', () => {
  beforeEach(() => openInShell.mockClear());

  it('opens a web address', async () => {
    expect(await openExternalUrl('https://example.com/a')).toBe(true);
    expect(openInShell).toHaveBeenCalledWith('https://example.com/a');
  });

  /**
   * These addresses come from search results and model tool calls. A page that
   * lists a `file:` link must not get it handed to the shell by a click.
   */
  it.each(['file:///etc/passwd', 'javascript:alert(1)', 'lattice://open', 'not a url'])(
    'refuses %s',
    async url => {
      expect(await openExternalUrl(url)).toBe(false);
      expect(openInShell).not.toHaveBeenCalled();
    }
  );
});
