import { describe, expect, it } from 'vitest';

import { FILE_TYPES } from '@/lib/bindings';

import { MIME_BY_EXTENSION, mimeTypeForPath } from '../mimeTypes';

describe('mimeTypeForPath', () => {
  it('maps the extensions the preview modal routes on', () => {
    expect(mimeTypeForPath('/vault/trial.pdf')).toBe('application/pdf');
    expect(mimeTypeForPath('/vault/notes.md')).toBe('text/markdown');
    expect(mimeTypeForPath('/vault/transcript.m4a')).toBe('audio/mp4');
  });

  it('ignores case and directory names, including dotted directories', () => {
    expect(mimeTypeForPath('/vault/PDF papers/Trial.PDF')).toBe('application/pdf');
    expect(mimeTypeForPath('/dir.with.dots/Makefile')).toBe('');
    expect(mimeTypeForPath('C:\\dir.with.dots\\Makefile')).toBe('');
  });

  it('matches the canonical Rust FILE_TYPES MIME mappings', () => {
    expect(mimeTypeForPath('/src/main.rs')).toBe('text/x-rust');
    expect(mimeTypeForPath('/include/header.h')).toBe('text/x-c-header');
    expect(mimeTypeForPath('/include/header.hpp')).toBe('text/x-c++-header');
    expect(mimeTypeForPath('/styles/app.less')).toBe('text/x-less');
    expect(mimeTypeForPath('/scripts/run.bash')).toBe('text/x-bash');
    expect(mimeTypeForPath('/scripts/env.zsh')).toBe('text/x-zsh');
  });

  it('matches every extension and mimeType in the backend FILE_TYPES catalogue', () => {
    for (const spec of FILE_TYPES) {
      expect(MIME_BY_EXTENSION[spec.extension.toLowerCase()]).toBe(spec.mimeType);
    }
  });

  it('returns an empty string for an unknown or missing extension', () => {
    expect(mimeTypeForPath('/vault/README')).toBe('');
    expect(mimeTypeForPath('/vault/archive.tar.zst')).toBe('');
    expect(mimeTypeForPath('/vault/trailing.')).toBe('');
    expect(mimeTypeForPath('')).toBe('');
  });
});
