import { describe, expect, it } from 'vitest';

import type { CompareTableDto } from '@/types/api/compare';

import { compareTableToMarkdown } from '../compareMarkdown';


function table(overrides: Partial<CompareTableDto> = {}): CompareTableDto {
  return {
    columns: ['method', 'sample size'],
    rows: [
      {
        documentId: 'doc_1',
        title: 'trial.pdf',
        filePath: '/vault/trial.pdf',
        cells: [
          { value: 'randomised controlled trial', citation: null },
          { value: '412', citation: null },
        ],
        error: null,
      },
      {
        documentId: 'doc_2',
        title: 'review.pdf',
        filePath: '/vault/review.pdf',
        cells: [
          { value: 'literature review', citation: null },
          { value: null, citation: null },
        ],
        error: null,
      },
    ],
    modelName: 'llama-3',
    generatedAt: '2026-09-06T12:00:00.000Z',
    ...overrides,
  };
}

describe('compareTableToMarkdown', () => {
  it('produces a header row, a separator row and one row per document', () => {
    const lines = compareTableToMarkdown(table()).split('\n').filter((l) => l.startsWith('|'));
    expect(lines).toHaveLength(4);
    expect(lines[0]).toBe('| Document | method | sample size |');
    expect(lines[1]).toBe('| --- | --- | --- |');
    expect(lines[2]).toContain('trial.pdf');
    expect(lines[3]).toContain('review.pdf');
  });

  it('escapes a pipe and collapses an embedded newline', () => {
    const source = table();
    source.rows[0].cells[0] = { value: 'a | b\nsecond line', citation: null };
    const markdown = compareTableToMarkdown(source);
    expect(markdown).toContain('a \\| b second line');
    // Only the unescaped pipes are structural, so the row still has 3 columns.
    const dataLine = markdown.split('\n').filter((l) => l.startsWith('| trial.pdf'))[0];
    const structuralPipes = (dataLine.match(/(^|[^\\])\|/g) ?? []).length;
    expect(structuralPipes).toBe(4);
  });

  it('escapes a backslash before a pipe so an escaped pipe stays escaped', () => {
    const source = table();
    source.rows[0].cells[0] = { value: 'a\\|b', citation: null };
    const markdown = compareTableToMarkdown(source);
    // The backslash is escaped first, so the pipe's own escape survives.
    expect(markdown).toContain('a\\\\\\|b');
    const dataLine = markdown.split('\n').filter((l) => l.startsWith('| trial.pdf'))[0];
    const structuralPipes = (dataLine.match(/(^|[^\\])\|/g) ?? []).length;
    expect(structuralPipes).toBe(4);
  });

  it('writes "not stated" for null cells', () => {
    const markdown = compareTableToMarkdown(table());
    expect(markdown).toContain('| review.pdf | literature review | not stated |');
  });

  it('writes an empty-string value as empty, not as "not stated"', () => {
    const source = table();
    source.rows[1].cells[1] = { value: '', citation: null };
    const markdown = compareTableToMarkdown(source);
    expect(markdown).toContain('| review.pdf | literature review |  |');
  });
});
