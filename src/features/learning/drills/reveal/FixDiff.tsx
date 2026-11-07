import type { DiffKind, DiffLine, FixDiffDto } from '@/lib/bindings';

const ROWS: Record<DiffKind, { mark: string; word: string; tone: string }> = {
  unchanged: { mark: '\u{a0}', word: 'Unchanged', tone: '' },
  removed: { mark: '-', word: 'Removed', tone: 'bg-danger-muted text-[color:var(--code-deleted)]' },
  added: { mark: '+', word: 'Added', tone: 'bg-success-muted text-[color:var(--code-inserted)]' },
};

function DiffRow({ line, prose }: { line: DiffLine; prose: boolean }) {
  const row = ROWS[line.kind];
  const Text = line.kind === 'added' ? 'ins' : line.kind === 'removed' ? 'del' : 'span';
  return (
    <div data-kind={line.kind} className={`flex gap-2 px-3 ${row.tone}`}>
      <span aria-hidden="true" className="w-3 shrink-0 select-none">{row.mark}</span>
      <Text className={prose ? 'min-w-0 whitespace-pre-wrap no-underline' : 'whitespace-pre no-underline'}>
        <span className="sr-only">{row.word}: </span>
        {line.text || '\u{a0}'}
      </Text>
    </div>
  );
}

/**
 * An issue's fix as the lines it removes and the lines it adds, in the code palette; for an explanation, as prose
 * that wraps.
 */
export function FixDiff({ fix, prose = false }: { fix: FixDiffDto; prose?: boolean }) {
  if (prose) {
    return (
      <figure className="overflow-hidden rounded-xl border border-border bg-bg">
        <div
          role="group"
          aria-label="The fix as a diff"
          className="py-1.5 text-[15px] leading-7 text-text-primary"
        >
          {fix.lines.map((line, index) => <DiffRow key={index} line={line} prose />)}
        </div>
      </figure>
    );
  }
  return (
    <figure className="overflow-hidden rounded-xl border border-border bg-[color:var(--code-bg)]">
      <figcaption className="border-b border-border px-3 py-1.5 font-mono text-xs text-text-muted">{fix.path}</figcaption>
      <div
        role="group"
        aria-label="The fix as a diff"
        className="overflow-x-auto py-1.5 font-mono text-[12.5px] leading-[1.65] text-[color:var(--code-fg)]"
      >
        <div className="min-w-max">
          {fix.lines.map((line, index) => <DiffRow key={index} line={line} prose={false} />)}
        </div>
      </div>
    </figure>
  );
}
