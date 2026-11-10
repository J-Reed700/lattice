import type * as Wire from '@/lib/bindings';

import type { CaseResultView } from '../shared/CaseResults';

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

/** "1:30" for ninety seconds. */
export function clockText(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

export const SETTING_NAMES: Record<Wire.CodingSetting, string> = {
  coached: 'Coached',
  mock: 'Mock',
  watched: 'Watched',
};

export const HELP_NAMES: Record<Wire.CodingSetting, string> = {
  coached: 'Full help',
  mock: 'Hints only',
  watched: 'Hints only',
};
