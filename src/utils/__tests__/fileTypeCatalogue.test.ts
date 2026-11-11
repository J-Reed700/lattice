import { describe, expect, it } from 'vitest';

import { FILE_TYPES } from '@/lib/bindings';

import { isSupportedFileType } from '../fileTypeDetector';
import { validateIndexablePath } from '../indexingFileValidation';

const indexable = FILE_TYPES.filter(format => format.route !== null);
const notIndexable = FILE_TYPES.filter(format => format.route === null);

describe('the generated file type catalogue', () => {
  it('lists each extension once', () => {
    const extensions = FILE_TYPES.map(format => format.extension);
    expect(new Set(extensions).size).toBe(extensions.length);
  });

  it.each(indexable.map(format => format.extension))('offers .%s for import and can open it', extension => {
    expect(validateIndexablePath(`file.${extension}`)).toEqual({ ok: true, extension });
    expect(isSupportedFileType(`file.${extension}`)).toBe(true);
  });

  it.each(notIndexable.map(format => format.extension))('does not offer .%s for import', extension => {
    expect(validateIndexablePath(`file.${extension}`).ok).toBe(false);
  });
});
