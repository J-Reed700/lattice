import { describe, expect, it } from 'vitest';

import { mimeTypeForPath } from '../mimeTypes';

describe('mimeTypeForPath', () => {
  it('maps the extensions the preview modal routes on', () => {
    expect(mimeTypeForPath('/vault/trial.pdf')).toBe('application/pdf');
    expect(mimeTypeForPath('/vault/notes.md')).toBe('text/markdown');
    expect(mimeTypeForPath('/vault/transcript.m4a')).toBe('audio/mp4');
  });

  it('ignores case and directory names', () => {
    expect(mimeTypeForPath('/vault/PDF papers/Trial.PDF')).toBe('application/pdf');
  });

  it('returns an empty string for an unknown or missing extension', () => {
    expect(mimeTypeForPath('/vault/README')).toBe('');
    expect(mimeTypeForPath('/vault/archive.tar.zst')).toBe('');
    expect(mimeTypeForPath('')).toBe('');
  });
});
