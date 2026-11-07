import type {
  CodeReviewFileDto,
  CodeReviewFindingDto,
  CodeReviewSeverity,
  CodeReviewSpanDto,
} from '@/lib/bindings';

/** One comment of the learner's review while they write it. `id` only keys it in the list; it is never sent. */
export interface DraftFinding {
  id: string;
  /** The lines it is on; none for a general comment. */
  span: CodeReviewSpanDto | null;
  text: string;
  severity: CodeReviewSeverity | null;
}

export type FindingInput = Omit<DraftFinding, 'id'>;

let issued = 0;

export function newFinding(input: FindingInput): DraftFinding {
  issued += 1;
  return { ...input, id: `finding-${Date.now().toString(36)}-${issued.toString(36)}` };
}

/** What is sent for a comment: its lines, its words and its severity. */
export function toFindingDto(finding: DraftFinding): CodeReviewFindingDto {
  return { span: finding.span, text: finding.text.trim(), severity: finding.severity };
}

/** `items` with the one at `index` moved one place up (`-1`) or down (`1`); the same order when it cannot move. */
export function moveItem<T>(items: readonly T[], index: number, delta: -1 | 1): T[] {
  const target = index + delta;
  if (index < 0 || index >= items.length || target < 0 || target >= items.length) return [...items];
  const moved = [...items];
  [moved[index], moved[target]] = [moved[target], moved[index]];
  return moved;
}

/** "Line 12 of api.py" or "Lines 12–14 of api.py"; for a passage, "Sentence 3" or "Sentences 3–4". */
export function describeSpan(span: CodeReviewSpanDto, material: 'code' | 'passage' = 'code'): string {
  const one = span.start === span.end;
  if (material === 'passage') return one ? `Sentence ${span.start}` : `Sentences ${span.start}–${span.end}`;
  const lines = one ? `Line ${span.start}` : `Lines ${span.start}–${span.end}`;
  return `${lines} of ${span.path}`;
}

/** How many lines a file has, counted as the backend counts the lines a comment may name. */
export function lineCount(text: string): number {
  if (text === '') return 0;
  const parts = text.split('\n');
  return text.endsWith('\n') ? parts.length - 1 : parts.length;
}

/** The files the learner has changed, with their text as they left it. */
export function changedFiles(
  files: readonly CodeReviewFileDto[],
  edits: Readonly<Record<string, string>>,
): CodeReviewFileDto[] {
  return files.flatMap((file) => {
    const text = edits[file.path];
    return text !== undefined && text !== file.text ? [{ path: file.path, text }] : [];
  });
}
