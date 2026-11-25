import type { CompareCellDto, CompareRowDto, CompareTableDto } from '@/types/api/compare';

interface CompareTableProps {
  table: CompareTableDto;
  onOpenCitation: (row: CompareRowDto, cell: CompareCellDto) => void;
}

/**
 * The table itself: hairline rules, no card, no zebra, no vertical rules.
 * A cell with no answer says "not stated" — never blank, never a dash — and
 * the sentence behind an answer is a whisper under it, not a badge beside it.
 */
export function CompareTable({ table, onOpenCitation }: CompareTableProps) {
  return (
    <div className="overflow-x-auto rounded-xl border border-border-subtle bg-surface shadow-sm">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="bg-surface-sunken/55">
            <th className="border-b border-border-subtle px-4 py-2.5 text-left font-sans text-xs font-semibold tracking-[-0.01em] text-text-secondary">
              Document
            </th>
            {table.columns.map((column) => (
              <th
                key={column}
                className="border-b border-border-subtle px-4 py-2.5 text-left font-sans text-xs font-semibold tracking-[-0.01em] text-text-secondary"
              >
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {table.rows.map((row) => (
            <tr key={row.documentId} className="group border-b border-border-subtle last:border-b-0">
              <td className="min-w-[180px] px-4 py-3.5 align-top font-sans font-semibold tracking-[-0.01em] text-text-primary">
                {row.title}
                {row.error ? (
                  <span className="mt-1 block text-xs font-normal text-text-muted">
                    {row.error}
                  </span>
                ) : null}
                {row.degraded ? (
                  <span className="mt-1 block text-xs font-normal text-text-muted">
                    {row.degraded}
                  </span>
                ) : null}
              </td>
              {table.columns.map((column, index) => {
                const cell = row.cells?.[index];
                // `null` is the backend's "the document does not say"; an empty
                // string is a value the model returned and is rendered as one.
                const hasValue = cell?.value !== null && cell?.value !== undefined;
                return (
                  <td
                    key={column}
                    className="px-4 py-3.5 align-top"
                  >
                    {hasValue ? (
                      <>
                        <span className="text-text-primary">{cell.value}</span>
                        {cell.citation ? (
                          <button
                            type="button"
                            onClick={() => onOpenCitation(row, cell)}
                            className="mt-1 block max-w-[34ch] truncate text-left text-xs text-text-muted transition-colors duration-fast hover:text-accent"
                            title={cell.citation.excerpt}
                          >
                            “{cell.citation.excerpt}”
                          </button>
                        ) : null}
                      </>
                    ) : (
                      <span className="text-text-muted">not stated</span>
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
