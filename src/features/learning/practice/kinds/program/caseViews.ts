import type * as Wire from '@/lib/bindings';

import type { CaseResultView } from '../../shared/CaseResults';

/** The case rows the shared results list shows, from exactly what the session's view holds. */
export function caseResultViews(cases: Wire.CaseViewDto[]): CaseResultView[] {
  return cases.map((item) => ({
    id: item.caseId,
    category: item.category,
    status: item.status,
    args: item.args,
    expected: item.expected,
    actual: item.actual,
    printed: item.printed,
    error: item.error,
    elapsedMs: item.elapsedMs,
    hidden: item.visibility === 'hidden' && !item.args && !item.expected && !item.actual,
    mine: item.mine,
  }));
}
