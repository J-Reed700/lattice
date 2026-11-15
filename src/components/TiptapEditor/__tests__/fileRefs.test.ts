import { describe, expect, it } from 'vitest';

import { fileRefFromHref } from '../extensions/fileRefs';

describe('fileRefFromHref', () => {
  it.each([
    ['src/main.rs#L12', 'src/main.rs', 12, 12],
    ['src/main.rs#L10-L24', 'src/main.rs', 10, 24],
    ['src-tauri/src/main.rs:64-67', 'src-tauri/src/main.rs', 64, 67],
    ['src/a.rs:12-4', 'src/a.rs', 4, 12],
    ['./lib/util.ts', 'lib/util.ts', null, null],
    ['file:///Users/me/proj/a.rs#L3', '/Users/me/proj/a.rs', 3, 3],
    ['src/my%20file.rs#L2', 'src/my file.rs', 2, 2],
  ])('reads the link address %s', (href, path, first, last) => {
    const ref = fileRefFromHref(href);
    expect(ref?.path).toBe(path);
    expect(ref?.lines?.startLine ?? null).toBe(first);
    expect(ref?.lines?.endLine ?? null).toBe(last);
  });

  it.each([
    'https://example.com/a.rs',
    'mailto:me@example.com',
    'http://localhost:5173/src/a.rs',
    '#section',
    'notes/rome',
    'src/a.rs:0',
  ])('leaves a link to %s alone', (href) => {
    expect(fileRefFromHref(href)).toBeNull();
  });
});
