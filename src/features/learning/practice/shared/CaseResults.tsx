export interface CaseResultView {
  id: string;
  category: string;
  status: 'passed' | 'failed' | 'error' | 'ran' | 'timed_out' | 'not_run';
  args?: string[] | null;
  expected?: string | null;
  actual?: string | null;
  printed?: string | null;
  error?: string | null;
  elapsedMs?: number | null;
  hidden?: boolean;
  mine?: boolean;
}

export function caseSummary(_cases: CaseResultView[]): string {
  return '';
}

export function CaseResults(_props: { cases: CaseResultView[]; title?: string; emptyText?: string }) {
  return null;
}
