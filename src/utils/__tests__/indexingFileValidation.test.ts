import { describe, expect, it } from 'vitest';

import { filterIndexablePaths, getExtensionFromPath, validateIndexablePath } from '../indexingFileValidation';

describe('indexing input validation', () => {
  it.each([
    ['/library.v2/café.MD', 'md'], ['C:\\Users\\Jøsh\\notes.PDF', 'pdf'],
    ['\\\\server\\share\\报告.TXT', 'txt'], ['archive.tar.gz', 'gz'],
    ['.config.json', 'json'], ['a..TsX', 'tsx'],
    ['', null], ['.gitignore', null], ['name.', null],
    ['/directory.ext/no-extension', null], ['/directory.ext/', null],
  ])('extracts the final filename extension from %s', (path, expected) => {
    expect(getExtensionFromPath(path)).toBe(expected);
  });

  it.each(['txt', 'md', 'pdf', 'docx', 'xlsx', 'pptx', 'rs', 'py', 'tsx', 'sql', 'json', 'csv', 'yml'])('accepts supported %s imports case-insensitively', extension => {
    expect(validateIndexablePath(`/資料/notes.${extension.toUpperCase()}`)).toEqual({ ok: true, extension });
  });

  it.each(['exe', 'dll', 'dylib', 'so', 'app', 'dmg', 'zip', 'tar', 'gz', '7z'])('rejects %s even behind a supported-looking prefix', extension => {
    expect(validateIndexablePath(`document.pdf.${extension.toUpperCase()}`)).toEqual({ ok: false, extension, reason: 'Executables and archives are not supported' });
  });

  it('distinguishes unknown types from filenames with no extension', () => {
    expect(validateIndexablePath('recording.unknown')).toEqual({ ok: false, extension: 'unknown', reason: 'Unsupported file type' });
    expect(validateIndexablePath('README')).toEqual({ ok: false, reason: 'Missing file extension' });
  });

  it('partitions a mixed batch without changing order, paths or duplicate identity', () => {
    const paths = ['/a.TXT', 'bad.exe', '/空白.md', 'README', '/a.TXT', 'image.webp'];
    const original = [...paths];
    const result = filterIndexablePaths(paths);
    expect(result.accepted).toEqual(['/a.TXT', '/空白.md', '/a.TXT']);
    expect(result.rejected).toEqual([
      { path: 'bad.exe', extension: 'exe', reason: 'Executables and archives are not supported' },
      { path: 'README', extension: undefined, reason: 'Missing file extension' },
      { path: 'image.webp', extension: 'webp', reason: 'Unsupported file type' },
    ]);
    expect(paths).toEqual(original);
    expect(filterIndexablePaths([])).toEqual({ accepted: [], rejected: [] });
  });
});
