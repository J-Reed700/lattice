import { describe, expect, it } from 'vitest';

import { formatSourceForReader } from '@/features/learning/sources/sourceReaderFormat';

describe('formatSourceForReader', () => {
  it('reflows legacy browser line wrapping and repairs extracted tables', () => {
    const saved = `# Build cache


Cargo stores build output in the target directory. By default,
this is the directory named target in the root of your
workspace.


 | Directory | Description


 | target/debug/ | Contains output for the dev profile.


 | target/release/ | Contains output for the release profile.


The cross target lives at target/<triple>/debug/.`;

    expect(formatSourceForReader(saved, 'web_reference_v1')).toBe(`# Build cache

Cargo stores build output in the target directory. By default, this is the directory named target in the root of your workspace.

| Directory | Description |
| --- | --- |
| target/debug/ | Contains output for the dev profile. |
| target/release/ | Contains output for the release profile. |

The cross target lives at target/&lt;triple&gt;/debug/.`);
  });

  it('does not reflow fenced code with blank lines or rewrite newer structured captures', () => {
    const markdown = '# Example\n\n```rust\nfn main() {\n\n    println!("hi");\n}\n```';
    expect(formatSourceForReader(markdown, 'web_reference_markdown_v2')).toBe(markdown);
    expect(formatSourceForReader(markdown, 'web_reference_v1')).toBe(markdown);
  });

  it('keeps <...> text as written when the reader draws the Markdown itself', () => {
    const saved = '# Build cache\n\nThe cross target lives at target/<triple>/debug/.\n\n| Path | Note\n\n| <dir>/x | uses <b>tags</b>\n\nSee <https://example.com/a>.';
    const kept = formatSourceForReader(saved, 'web_reference_v1', { escapeMarkup: false });
    expect(kept).toBe(
      '# Build cache\n\nThe cross target lives at target/<triple>/debug/.\n\n| Path | Note |\n| --- | --- |\n| <dir>/x | uses <b>tags</b> |\n\nSee <https://example.com/a>.',
    );
    expect(kept).not.toContain('&lt;');
    // The default still escapes for TiptapViewer, and leaves addresses alone.
    const escaped = formatSourceForReader(saved, 'web_reference_v1');
    expect(escaped).toContain('target/&lt;triple&gt;/debug/');
    expect(escaped).toContain('| &lt;dir&gt;/x | uses &lt;b&gt;tags&lt;/b&gt; |');
    expect(escaped).toContain('<https://example.com/a>');
  });

  it('leaves newer captures unchanged either way', () => {
    const markdown = 'A <x> here.';
    expect(formatSourceForReader(markdown, 'web_reference_markdown_v2', { escapeMarkup: false })).toBe(markdown);
    expect(formatSourceForReader(markdown, 'web_reference_markdown_v2')).toBe(markdown);
  });
});
