import { resolveExtensions } from '@tiptap/core';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';

describe('createExtensions', () => {
  it('registers every extension name exactly once', () => {
    // StarterKit bundles its own Link (and others) since tiptap 3; a second
    // registration makes tiptap warn "Duplicate extension names found" on
    // every editor mount and can double-apply input rules.
    const names = resolveExtensions(createExtensions()).map((extension) => extension.name);
    const duplicates = names.filter((name, index) => names.indexOf(name) !== index);
    expect(duplicates).toEqual([]);
  });

  it('registers every extension name exactly once with the slash menu enabled', () => {
    // The editable mount passes `slashMenu`; the guard above only covers the
    // read-only extension set, so the `/` extension needs its own check.
    const names = resolveExtensions(
      createExtensions({
        placeholder: 'Write…',
        slashMenu: { onStateChange: () => {}, onKeyDown: () => false },
      }),
    ).map((extension) => extension.name);
    const duplicates = names.filter((name, index) => names.indexOf(name) !== index);
    expect(duplicates).toEqual([]);
    expect(names).toContain('slashMenu');
  });

  it('keeps link and code block after disabling the StarterKit copies', () => {
    const names = resolveExtensions(createExtensions()).map((extension) => extension.name);
    expect(names.filter((name) => name === 'link')).toHaveLength(1);
    expect(names.filter((name) => name === 'codeBlock')).toHaveLength(1);
  });
});
