import { describe, expect, it } from 'vitest';

import { parseImages } from '@/features/chat/controller/messageMetadata';

const image = {
  id: '0123456789abcdef0123456789abcdef',
  ref: 'img-1',
  fileName: '0123456789abcdef0123456789abcdef.jpg',
  mime: 'image/jpeg',
  width: 1280,
  height: 853,
  byteSize: 204800,
  caption: 'Saturn V on the pad',
  sourceUrl: 'https://commons.wikimedia.org/wiki/File:Saturn_V.jpg',
  sourceTitle: 'File:Saturn_V.jpg',
  origin: 'wikimedia_commons',
  license: 'Public domain',
  attribution: null,
};

describe('chat picture records', () => {
  it('keeps a picture whose source is a web page', () => {
    expect(parseImages([image])).toHaveLength(1);
  });

  it('drops a picture whose source link would run script', () => {
    const pictures = parseImages([image, { ...image, ref: 'img-2', sourceUrl: 'javascript:alert(1)' }]);
    expect(pictures.map((picture) => picture.ref)).toEqual(['img-1']);
  });
});
