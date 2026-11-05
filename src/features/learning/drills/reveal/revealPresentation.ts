import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import { joinList } from '@/features/learning/drills/reveal/revealCopy';
import { SEVERITIES } from '@/features/learning/drills/severity';
import type {
  CodeReviewFileDto,
  CodeReviewRevealDto,
  RevealedFindingDto,
  RevealedIssueDto,
  SuiteVerdict,
} from '@/lib/bindings';

export interface RankedIssue {
  issue: RevealedIssueDto;
  /** Where the key holds the issue: what findings and fix results name it by. */
  position: number;
  /** Where the page shows the issue, from 1: most severe first. */
  number: number;
}

export function rankIssues(issues: readonly RevealedIssueDto[]): RankedIssue[] {
  const order = (issue: RevealedIssueDto) => SEVERITIES.indexOf(issue.severity);
  return issues
    .map((issue, position) => ({ issue, position }))
    .sort((a, b) => order(a.issue) - order(b.issue) || a.position - b.position)
    .map((entry, index) => ({ ...entry, number: index + 1 }));
}

export function numberOf(position: number, ranked: readonly RankedIssue[]): number {
  return ranked.find((entry) => entry.position === position)?.number ?? Number.MAX_SAFE_INTEGER;
}

/** What a finding matches: "Matches issue 2, Check-then-act race". */
export function matchLabel(finding: RevealedFindingDto, ranked: readonly RankedIssue[]): string {
  const refs = finding.issues.map((position) => {
    const entry = ranked.find((candidate) => candidate.position === position);
    return entry ? `issue ${entry.number}, ${entry.issue.patternName}` : `issue ${position + 1}`;
  });
  return refs.length > 0 ? `Matches ${joinList(refs)}` : 'Matches the key';
}

/** Issues by key position, as the page numbers them: "Magic number (issue 3) and Stale state (issue 1)". */
export function issuesNamed(positions: readonly number[], ranked: readonly RankedIssue[]): string {
  return joinList(
    [...positions]
      .sort((a, b) => numberOf(a, ranked) - numberOf(b, ranked))
      .map((position) => {
        const entry = ranked.find((candidate) => candidate.position === position);
        return entry ? `${entry.issue.patternName} (issue ${entry.number})` : `issue ${position + 1}`;
      }),
  );
}

export interface LeadingLine {
  led: boolean;
  text: string;
}

const was = (items: readonly unknown[]) => (items.length === 1 ? 'was' : 'were');

/** Whether the review led with the key's most severe issue, and the short reason. */
export function leadingLine(reveal: CodeReviewRevealDto): LeadingLine | null {
  const top = SEVERITIES.find((severity) => reveal.issues.some((issue) => issue.severity === severity));
  if (!top) return null;
  const tops = reveal.issues.filter((issue) => issue.severity === top);
  const named = (list: readonly RevealedIssueDto[]) => joinList(list.map((issue) => issue.patternName));
  if (reveal.score.ledWithTheMostSevere) {
    const first = tops.filter((issue) => issue.outcome === 'found' && issue.finding === 0);
    return {
      led: true,
      text: first.length > 0
        ? `Led with the most severe issue: your first comment found ${named(first)}.`
        : 'Led with the most severe issue.',
    };
  }
  const lead = 'Did not lead with the most severe issue:';
  if (reveal.findings.length === 0) return { led: false, text: `${lead} you left no comments.` };
  const later = tops.filter((issue) => issue.outcome === 'found' && (issue.finding ?? 0) > 0);
  if (later.length > 0) {
    const comment = Math.min(...later.map((issue) => issue.finding ?? 0)) + 1;
    return { led: false, text: `${lead} ${named(later)} ${was(later)} found by your comment ${comment}, not your first.` };
  }
  const half = tops.filter((issue) => issue.outcome === 'half_found');
  if (half.length > 0) {
    return { led: false, text: `${lead} ${named(half)} ${was(half)} only half found: a comment pointed at it but gave no cause, or the wrong one.` };
  }
  return { led: false, text: `${lead} you missed ${named(tops)}.` };
}

/**
 * Where an issue sits in the clean code: the lines its fix leaves, found in the clean file by their text, the run
 * nearest the issue's lines when the text repeats. None when the fix only removes lines.
 */
export function cleanLinesOf(
  issue: RevealedIssueDto,
  cleanFiles: readonly CodeReviewFileDto[],
): ExplorerLineRange | null {
  const file = cleanFiles.find((candidate) => candidate.path === issue.fix.path);
  const wanted = issue.fix.lines.filter((line) => line.kind !== 'removed').map((line) => line.text);
  if (!file || wanted.length === 0) return null;
  const lines = file.text.split(/\r?\n/);
  let best: number | null = null;
  for (let start = 0; start + wanted.length <= lines.length; start += 1) {
    if (!wanted.every((text, offset) => lines[start + offset] === text)) continue;
    if (best === null || Math.abs(start + 1 - issue.span.start) < Math.abs(best + 1 - issue.span.start)) best = start;
  }
  return best === null ? null : { startLine: best + 1, endLine: best + wanted.length };
}

/** The files to choose between: the reviewed code's, then any the clean code alone has. */
export function contrastPaths(
  reviewed: readonly CodeReviewFileDto[],
  clean: readonly CodeReviewFileDto[],
): string[] {
  return [...new Set([...reviewed, ...clean].map((file) => file.path))];
}

export interface SuiteSummary {
  verdict: SuiteVerdict['status'];
  headline: string;
  detail: string | null;
  output: string | null;
}

export function suiteSummary(suite: SuiteVerdict, ranked: readonly RankedIssue[]): SuiteSummary {
  const ranWith = (positions: readonly number[]) =>
    positions.length > 0
      ? `It ran with the fixes of the issues you left: ${issuesNamed(positions, ranked)}.`
      : 'It ran on your files as you left them.';
  switch (suite.status) {
    case 'passed':
      return {
        verdict: 'passed',
        headline: 'The hidden suite passes on your files: nothing that worked is broken.',
        detail: ranWith(suite.ranBeside),
        output: null,
      };
    case 'failed':
      return {
        verdict: 'failed',
        headline: 'The hidden suite fails on your files: your changes broke something that worked.',
        detail: ranWith(suite.ranBeside),
        output: suite.output,
      };
    case 'not_run':
      return { verdict: 'not_run', headline: 'The hidden suite did not run.', detail: suite.why, output: null };
  }
}
