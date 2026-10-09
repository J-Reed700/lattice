import { describe, expect, it } from 'vitest';

import { detectFileType, isSupportedFileType } from '../fileTypeDetector';

describe('detectFileType', () => {
  it.each([
    ['README.MD', { type: 'markdown', canPreview: true }],
    ['/repo/component.TSX', { type: 'code', language: 'typescript', canPreview: true }],
    ['/repo/script.py', { type: 'code', language: 'python', canPreview: true }],
    ['/repo/.gitignore', { type: 'text', canPreview: true }],
    ['/repo/book.odt', { type: 'text', canPreview: true }],
    ['/repo/sheet.xlsx', { type: 'text', canPreview: true }],
    ['/repo/slides.pptx', { type: 'text', canPreview: true }],
    ['/repo/page.HTM', { type: 'html', canPreview: true }],
    ['/repo/report.docx', { type: 'docx', canPreview: true }],
    ['/repo/report.PDF', { type: 'pdf', canPreview: true }],
    ['/repo/photo.WebP', { type: 'image', canPreview: true }],
    ['/repo/archive.zip', { type: 'unsupported', canPreview: false }],
    ['/repo/LICENSE', { type: 'unsupported', canPreview: false }],
    ['/repo/trailing.', { type: 'unsupported', canPreview: false }],
  ] as const)('classifies %s', (path, expected) => {
    expect(detectFileType(path)).toEqual(expected);
  });

  it('uses the last suffix when a filename contains several dots', () => {
    expect(detectFileType('/tmp/archive.tar.json')).toEqual({
      type: 'code',
      language: 'json',
      canPreview: true,
    });
  });
});

describe('isSupportedFileType', () => {
  it('reflects whether the detector has a preview implementation', () => {
    expect(isSupportedFileType('notes.md')).toBe(true);
    expect(isSupportedFileType('archive.7z')).toBe(false);
  });
});
