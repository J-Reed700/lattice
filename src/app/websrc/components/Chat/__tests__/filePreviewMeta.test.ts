import { describe, expect, it } from 'vitest';

import { passageMatchNotice, sourceHeaderMeta } from '../filePreviewMeta';

describe('sourceHeaderMeta', () => {
  it('says the size when the file was actually measured', () => {
    expect(sourceHeaderMeta({ category: 'PDF', fileSizeBytes: 2048 })).toEqual([
      'PDF',
      '2.0 KB',
    ]);
  });

  it('never renders "0.0 B" for a source carrying no size', () => {
    // Passage references and compare cells build a SourceWithMetadata with no
    // size; the old header claimed every one of them was 0.0 B.
    const meta = sourceHeaderMeta({ category: 'PDF', fileSizeBytes: 0 });
    expect(meta).toEqual(['PDF']);
    expect(meta.join(' · ')).not.toContain('B');
  });

  it('returns nothing at all when neither value is known', () => {
    expect(sourceHeaderMeta({ category: '', fileSizeBytes: 0 })).toEqual([]);
    expect(sourceHeaderMeta({ category: '   ', fileSizeBytes: 0 })).toEqual([]);
  });
});

describe('passageMatchNotice', () => {
  it('admits an approximate landing', () => {
    expect(passageMatchNotice('approximate', true)).toBe(
      'Approximate position — the file changed since it was indexed.'
    );
  });

  it('admits a passage it could not find', () => {
    expect(passageMatchNotice('none', true)).toBe(
      "Couldn't find this passage in the file."
    );
  });

  it('says nothing on an exact match, or when there is no citation to land on', () => {
    expect(passageMatchNotice('exact', true)).toBeNull();
    expect(passageMatchNotice('none', false)).toBeNull();
  });
});
