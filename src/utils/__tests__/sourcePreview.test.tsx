import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  getSourceExternalUrl,
  getSourcePreviewKind,
  getWebArchiveHtmlPath,
  isHttpUrl,
  isWebArchiveArticlePath,
  renderHighlightedText,
  selectInformativeTerms,
  termSalience,
} from '../sourcePreview';

describe('source URLs', () => {
  it.each([
    [' https://example.com/a ', true],
    ['HTTP://EXAMPLE.COM', true],
    ['ftp://example.com', false],
    ['example.com/path', false],
    ['', false],
  ] as const)('recognizes HTTP URL %j as %s', (value, expected) => {
    expect(isHttpUrl(value)).toBe(expected);
  });

  it('uses the first usable source field and adds HTTPS to host paths', () => {
    expect(getSourceExternalUrl({
      filePath: '/local/file.md',
      path: 'docs.example.com/guide',
      documentId: 'web:https%3A%2F%2Ffallback.example.com',
    })).toBe('https://docs.example.com/guide');
  });

  it('decodes a web document ID when paths are unavailable', () => {
    expect(getSourceExternalUrl({ documentId: 'web:https%3A%2F%2Fexample.com%2Fa%3Fb%3D1' }))
      .toBe('https://example.com/a?b=1');
  });

  it('unwraps a different-host redirect target, including double encoding', () => {
    expect(getSourceExternalUrl({
      filePath: 'https://redirect.example/away?target=https%253A%252F%252Fdocs.example%252Fguide',
    })).toBe('https://docs.example/guide');
  });

  it('skips empty, invalid, and same-host redirect values before a valid target', () => {
    expect(getSourceExternalUrl({
      filePath: 'https://redirect.example/away?empty=&bad=not-a-url&same=https%3A%2F%2Fredirect.example%2Fnext&url=https%3A%2F%2Fdestination.example%2F',
    })).toBe('https://destination.example/');
  });

  it('keeps a wrapper URL when it has no external target', () => {
    const wrapper = 'https://redirect.example/away?next=https%3A%2F%2Fredirect.example%2Fnext';
    expect(getSourceExternalUrl({ filePath: wrapper })).toBe(wrapper);
  });

  it('does not throw on malformed encoded IDs or URLs', () => {
    expect(getSourceExternalUrl({ documentId: 'web:%E0%A4%A' })).toBeNull();
    expect(getSourceExternalUrl({ filePath: 'http://%' })).toBe('http://%');
    expect(getSourceExternalUrl({
      filePath: 'https://redirect.example/?target=http%3A%2F%2F%25',
    })).toBe('https://redirect.example/?target=http%3A%2F%2F%25');
  });

  it('returns null when none of the source fields contain a URL', () => {
    expect(getSourceExternalUrl({ filePath: '/tmp/note.md', path: 'notes', documentId: 'local:1' }))
      .toBeNull();
  });
});

describe('archived web sources', () => {
  it.each([
    ['/Users/me/.lattice/web-archive/id/article.md', true],
    [' /USERS/ME/.LATTICE/WEB-ARCHIVE/ID/WEBLINK.MD ', true],
    ['/Users/me/.lattice/web-archive/id/notes.md', false],
    ['/Users/me/article.md', false],
    ['', false],
  ] as const)('classifies %j as archived=%s', (path, expected) => {
    expect(isWebArchiveArticlePath(path)).toBe(expected);
  });

  it('maps an archived markdown record to its captured HTML sibling', () => {
    expect(getWebArchiveHtmlPath('/Users/me/.lattice/web-archive/id/article.md'))
      .toBe('/Users/me/.lattice/web-archive/id/page.html');
  });

  it('does not produce an HTML path for an ordinary markdown file', () => {
    expect(getWebArchiveHtmlPath('/Users/me/notes/article.md')).toBeNull();
  });
});

describe('preview kind', () => {
  it('gives archived pages precedence over their local markdown path', () => {
    expect(getSourcePreviewKind({
      filePath: '/Users/me/.lattice/web-archive/id/article.md',
      path: 'https://example.com',
    })).toBe('archived-web');
  });

  it.each([
    [{ filePath: 'https://example.com' }, 'external-web'],
    [{ path: 'example.com/story' }, 'external-web'],
    [{ category: 'Saved_Web-Article', filePath: 'story-1' }, 'external-web'],
    [{ mimeType: 'TEXT/HTML', filePath: 'story-1' }, 'external-web'],
    [{ category: 'web article', filePath: '/Users/me/article.html' }, 'local-file'],
    [{ category: 'web article', filePath: 'C:\\docs\\article.html' }, 'local-file'],
    [{ mimeType: 'text/html', filePath: '\\\\server\\share\\article.html' }, 'local-file'],
    [{ filePath: '/Users/me/notes.md' }, 'local-file'],
  ] as const)('classifies %j as %s', (source, expected) => {
    expect(getSourcePreviewKind(source)).toBe(expected);
  });
});

describe('search term highlighting', () => {
  it('scores empty and repetitive terms as zero', () => {
    expect(termSalience('')).toBe(0);
    expect(termSalience('aaaa')).toBe(0);
    expect(termSalience('lattice')).toBeGreaterThan(0);
  });

  it('normalizes, deduplicates, filters, ranks, and caps terms', () => {
    const selected = selectInformativeTerms(
      ['  Lattice ', 'lattice', 'of', '1234', 'retrieval', 'aaaa'],
      2,
    );

    expect(selected).toHaveLength(2);
    expect(new Set(selected)).toEqual(new Set(['lattice', 'retrieval']));
  });

  it('keeps zero-entropy alphabetic terms when those are the only choices', () => {
    expect(selectInformativeTerms(['aaa', 'bbb'], 1)).toEqual(['aaa']);
  });

  it('returns no terms when every candidate is too short or nonalphabetic', () => {
    expect(selectInformativeTerms(['a', '12', ' --- '], 10)).toEqual([]);
  });

  it('returns plain text when highlights are absent or uninformative', () => {
    expect(renderHighlightedText('Plain excerpt')).toBe('Plain excerpt');
    expect(renderHighlightedText('Plain excerpt', ['a', '12'])).toBe('Plain excerpt');
  });

  it('highlights whole words case-insensitively and escapes regex punctuation', () => {
    render(<>{renderHighlightedText(
      'Node.js and NODE.JS differ from nodeXjs; lattice appears twice: LATTICE.',
      ['node.js', 'lattice'],
    )}</>);

    expect(screen.getAllByText(/node\.js/i)).toHaveLength(2);
    expect(screen.getAllByText(/lattice/i)).toHaveLength(2);
    expect(screen.getByText(/nodeXjs/)).toBeInTheDocument();
    expect(document.querySelectorAll('mark')).toHaveLength(4);
  });
});
