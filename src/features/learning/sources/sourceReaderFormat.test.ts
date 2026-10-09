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
});
