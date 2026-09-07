import type { CompareTableDto } from '@/types/api/compare';

/** A cell that says nothing says so, in the table's own words. */
const NOT_STATED = 'not stated';

/**
 * A markdown table's cells cannot contain a pipe or a newline, so both are
 * neutralised rather than allowed to break the table.
 *
 * Backslashes go first: a cell holding `a\|b` escaped pipe-first becomes
 * `a\\|b`, where the pair reads as one escaped backslash and the pipe goes
 * back to being structural — one cell's text would split the row.
 */
function escapeCell(value: string): string {
  return value
    .replace(/\\/g, '\\\\')
    .replace(/\|/g, '\\|')
    .replace(/\s*\n+\s*/g, ' ')
    .trim();
}

/**
 * Renders a compare table as a markdown block for the journal.
 *
 * Citations are deliberately left out: a quote in every cell is unreadable,
 * and the journal page's job is the shape of the answer, not the evidence.
 */
export function compareTableToMarkdown(table: CompareTableDto): string {
  const generatedAt = new Date(table.generatedAt);
  const stamp = Number.isNaN(generatedAt.getTime())
    ? table.generatedAt
    : generatedAt.toLocaleString();

  const header = ['Document', ...table.columns.map(escapeCell)];
  const separator = header.map(() => '---');

  const rows = table.rows.map((row) => {
    const cells = table.columns.map((_column, index) => {
      const value = row.cells?.[index]?.value;
      return value === null || value === undefined ? NOT_STATED : escapeCell(value);
    });
    return [escapeCell(row.title), ...cells];
  });

  const toLine = (cells: string[]) => `| ${cells.join(' | ')} |`;

  return [
    `## Comparison · ${table.columns.join(', ')}`,
    `_Generated ${stamp} across ${table.rows.length} ${
      table.rows.length === 1 ? 'document' : 'documents'
    }._`,
    '',
    toLine(header),
    toLine(separator),
    ...rows.map(toLine),
    '',
  ].join('\n');
}
