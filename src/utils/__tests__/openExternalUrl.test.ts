import { beforeEach, describe, expect, it, vi } from 'vitest';

import { openExternalUrl, whyExternalUrlDidNotOpen } from '../openExternalUrl';

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

describe('whyExternalUrlDidNotOpen', () => {
  beforeEach(() => openInShell.mockReset());

  it('says nothing when the address opened', async () => {
    expect(await whyExternalUrlDidNotOpen('https://example.com/a')).toBeNull();
  });

  it("gives the shell's reason when neither the shell nor a new window opens it", async () => {
    openInShell.mockRejectedValueOnce(new Error('shell blocked'));
    const open = vi.spyOn(window, 'open').mockImplementation(() => null);
    expect(await whyExternalUrlDidNotOpen('https://example.com/a')).toBe('shell blocked');
    open.mockRestore();
  });

  it('says nothing when the shell fails but a new window opens it', async () => {
    openInShell.mockRejectedValueOnce(new Error('shell blocked'));
    const open = vi.spyOn(window, 'open').mockImplementation(() => ({}) as Window);
    expect(await whyExternalUrlDidNotOpen('https://example.com/a')).toBeNull();
    open.mockRestore();
  });

  it('says why an address that is not http or https is refused', async () => {
    expect(await whyExternalUrlDidNotOpen('file:///etc/passwd')).toBe('Only http and https addresses open in the browser.');
    expect(openInShell).not.toHaveBeenCalled();
  });
});
