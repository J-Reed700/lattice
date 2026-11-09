export type CaseRowStatus = 'passed' | 'failed' | 'error' | 'ran' | 'timed_out' | 'not_run';

export interface CaseRowItem {
  id: string;
  name: string;
  status: CaseRowStatus;
  args?: string[] | null;
  expected?: string | null;
  actual?: string | null;
  printed?: string | null;
  error?: string | null;
  durationMs?: number | null;
  message?: string | null;
}

export interface CaseResultsProps {
  cases: CaseRowItem[];
  stdout?: string;
  stderr?: string;
  summary?: string;
  className?: string;
}

export function CaseResults(_props: CaseResultsProps) {
  return <></>;
}
