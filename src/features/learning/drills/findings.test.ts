import { describe, expect, it } from 'vitest';

import { changedFiles, describeSpan, lineCount, moveItem, newFinding, toFindingDto } from './findings';

describe('moving a comment', () => {
  it('moves one place up or down and leaves the others in order', () => {
    expect(moveItem(['a', 'b', 'c'], 2, -1)).toEqual(['a', 'c', 'b']);
    expect(moveItem(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c']);
  });

  it('does not move the first up, the last down or one that is not there, and never changes the list it was given', () => {
    const items = ['a', 'b', 'c'];
    expect(moveItem(items, 0, -1)).toEqual(items);
    expect(moveItem(items, 2, 1)).toEqual(items);
    expect(moveItem(items, 7, -1)).toEqual(items);
    moveItem(items, 1, 1);
    expect(items).toEqual(['a', 'b', 'c']);
  });
});

describe('describing lines', () => {
  it('says one line or a range, in a file', () => {
    expect(describeSpan({ path: 'api.py', start: 12, end: 12 })).toBe('Line 12 of api.py');
    expect(describeSpan({ path: 'api.py', start: 12, end: 14 })).toBe('Lines 12–14 of api.py');
  });

  it('counts the lines a comment may name as the backend does', () => {
    expect(lineCount('')).toBe(0);
    expect(lineCount('a')).toBe(1);
    expect(lineCount('a\n')).toBe(1);
    expect(lineCount('a\n\n')).toBe(2);
    expect(lineCount('a\nb')).toBe(2);
    expect(lineCount('a\nb\n')).toBe(2);
  });
});

describe('what is sent for a comment', () => {
  it('is its lines, its trimmed words and its severity, without the id that keys it in the list', () => {
    const finding = newFinding({ span: { path: 'api.py', start: 3, end: 4 }, text: '  Not atomic.  ', severity: 'major' });
    expect(toFindingDto(finding)).toEqual({ span: { path: 'api.py', start: 3, end: 4 }, text: 'Not atomic.', severity: 'major' });
    expect(Object.keys(toFindingDto(finding)).sort()).toEqual(['severity', 'span', 'text']);
  });

  it('gives every comment its own id', () => {
    const input = { span: null, text: 'Same words.', severity: null };
    expect(newFinding(input).id).not.toBe(newFinding(input).id);
  });
});

describe('the files a fix changed', () => {
  const files = [
    { path: 'a.py', text: 'a = 1\n' },
    { path: 'b.py', text: 'b = 1\n' },
  ];

  it('are the ones whose text differs, in the drill order', () => {
    expect(changedFiles(files, { 'b.py': 'b = 2\n', 'a.py': 'a = 2\n' })).toEqual([
      { path: 'a.py', text: 'a = 2\n' },
      { path: 'b.py', text: 'b = 2\n' },
    ]);
  });

  it('are none when nothing differs or the learner never opened a file', () => {
    expect(changedFiles(files, {})).toEqual([]);
    expect(changedFiles(files, { 'a.py': 'a = 1\n' })).toEqual([]);
    expect(changedFiles(files, { 'elsewhere.py': 'x' })).toEqual([]);
  });
});
