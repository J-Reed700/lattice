import { describe, expect, it } from 'vitest';

import { isRemoteFileSource, resolveLocalPathFromViewerSource } from '../fileSources';

describe('isRemoteFileSource', () => {
  it.each([
    ['https://example.com/report.pdf', true],
    ['HTTP://example.com/report.pdf', true],
    ['data:application/pdf;base64,AA==', true],
    ['blob:https://example.com/id', true],
    ['https://asset.localhost/tmp/report.pdf', false],
    ['HTTP://ASSET.LOCALHOST/tmp/report.pdf', false],
    ['asset://localhost/tmp/report.pdf', false],
    ['/tmp/report.pdf', false],
  ] as const)('classifies %s as remote=%s', (source, expected) => {
    expect(isRemoteFileSource(source)).toBe(expected);
  });
});

describe('resolveLocalPathFromViewerSource', () => {
  it.each([
    [' asset://localhost/Users/me/My%20File.pdf ', '/Users/me/My File.pdf'],
    ['tauri://localhost/C%3A%5Cdocs%5Creport.pdf', '/C:\\docs\\report.pdf'],
    ['https://asset.localhost/tmp/report.pdf', '/tmp/report.pdf'],
    ['http://asset.localhost/relative/report.pdf', '/relative/report.pdf'],
    ['file:///Users/me/My%20File.pdf', '/Users/me/My File.pdf'],
    ['/already/local.pdf', '/already/local.pdf'],
    [' relative.pdf ', 'relative.pdf'],
  ] as const)('resolves %s to %s', (source, expected) => {
    expect(resolveLocalPathFromViewerSource(source)).toBe(expected);
  });

  it('keeps malformed asset escapes while still returning an absolute path', () => {
    expect(resolveLocalPathFromViewerSource('asset://localhost/tmp/%E0%A4%A.pdf')).toBe('/tmp/%E0%A4%A.pdf');
  });

  it('returns malformed file URLs unchanged', () => {
    expect(resolveLocalPathFromViewerSource('file://%')).toBe('file://%');
  });
});
