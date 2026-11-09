import type { CodingExampleDto } from './codingApi';

export interface ExamplesTableProps {
  examples: CodingExampleDto[];
  totalMisreadings: number;
  onCheckExample: (argsJson: string, expectedJson: string) => Promise<void> | void;
  checking?: boolean;
  paramNames?: string[];
  className?: string;
}

/** ExamplesTable - "Try an example first" table, checking form, verdict badge, and misreading coverage. Minimal non-crashing stub for Stage 1. */
export function ExamplesTable(_props: ExamplesTableProps) {
  return <section aria-label="Examples" data-testid="stub-examples-table" />;
}
