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
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr>
            <th className="border-b border-border-subtle pb-2 pr-6 text-left font-medium text-text-secondary">
              Document
            </th>
            {table.columns.map((column) => (
              <th
                key={column}
                className="border-b border-border-subtle pb-2 pr-6 text-left font-medium text-text-secondary"
              >
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {table.rows.map((row) => (
            <tr key={row.documentId}>
              <td className="min-w-[180px] border-b border-border-subtle py-3 pr-6 align-top font-medium text-text-primary">
                {row.title}
                {row.error ? (
                  <span className="mt-1 block text-xs font-normal text-text-muted">
                    {row.error}
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
                    className="border-b border-border-subtle py-3 pr-6 align-top"
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
