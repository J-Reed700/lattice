import type { TurnStep, TurnStepKind } from '@/types/conversation';

/**
 * A turn's steps, split where the model went back for more.
 *
 * Retrieval gathers once before the model runs. A model with tools can then
 * read what came back, decide it is not enough, and go looking again — and
 * again. In a flat list that second trip is a few more "Reading …" rows under
 * a "Thinking" row, which reads as one long search rather than as the model
 * choosing to dig. Naming the trips is the whole point of showing them.
 *
 * A round is a run of gathering steps with nothing but gathering in it; the
 * generation that asked for the next run is what separates them. A turn that
 * only looked once has no rounds worth naming and comes back as one section.
 */

const GATHERING: ReadonlySet<TurnStepKind> = new Set<TurnStepKind>([
  'plan',
  'search_documents',
  'sufficiency',
  'corrective_search',
  'web_search',
  'read_page',
  'wiki',
  'open_document',
  'tool',
]);

const SEARCHES: ReadonlySet<TurnStepKind> = new Set<TurnStepKind>([
  'search_documents',
  'corrective_search',
  'web_search',
  'wiki',
]);

export interface TurnSection {
  /** 1-based, or `null` for steps that belong to no round. */
  round: number | null;
  steps: TurnStep[];
}

export function isGathering(step: TurnStep): boolean {
  return GATHERING.has(step.kind);
}

export interface StepRun {
  gathering: boolean;
  steps: TurnStep[];
}

/**
 * The steps cut where gathering starts and stops, and nothing more decided.
 * Both the one-line record and the research deck are built on this cut, so
 * they cannot disagree about where a round begins.
 */
export function runsOf(steps: readonly TurnStep[]): StepRun[] {
  const runs: StepRun[] = [];
  for (const step of steps) {
    const gathering = isGathering(step);
    const last = runs[runs.length - 1];
    if (last?.gathering === gathering) last.steps.push(step);
    else runs.push({ gathering, steps: [step] });
  }
  return runs;
}

export function sectionsOf(steps: readonly TurnStep[]): TurnSection[] {
  let rounds = 0;
  const sections = runsOf(steps).map(run => ({
    round: run.gathering ? (rounds += 1) : null,
    steps: run.steps,
  }));
  // One trip is just "what the turn did". Numbering it would promise a second.
  if (rounds < 2) return [{ round: null, steps: [...steps] }];
  return sections;
}

export function roundCount(steps: readonly TurnStep[]): number {
  return sectionsOf(steps).filter(section => section.round !== null).length;
}

/** Which round a step fell in, for the one-line view of a running turn. */
export function roundOf(steps: readonly TurnStep[], stepId: string): number | null {
  for (const section of sectionsOf(steps)) {
    if (section.steps.some(step => step.id === stepId)) return section.round;
  }
  return null;
}

const plural = (count: number, one: string, many: string) =>
  `${count} ${count === 1 ? one : many}`;

/**
 * What a round came to, counted from its own steps.
 *
 * A page that refused is said out loud: "read 2 of 5" is the honest account of
 * a round, and the blocked sites are usually why the model went back again.
 */
export function roundSummary(steps: readonly TurnStep[]): string {
  const parts: string[] = [];
  const searches = steps.filter(step => SEARCHES.has(step.kind)).length;
  const pages = steps.filter(step => step.kind === 'read_page');
  const pagesRead = pages.filter(step => step.state === 'done').length;
  const pagesRefused = pages.filter(step => step.state === 'failed').length;
  const documents = steps.filter(
    step => step.kind === 'open_document' && step.state === 'done'
  ).length;
  if (searches > 0) parts.push(plural(searches, 'search', 'searches'));
  if (pagesRead > 0) parts.push(`${plural(pagesRead, 'page', 'pages')} read`);
  if (pagesRefused > 0) parts.push(`${pagesRefused} could not be read`);
  if (documents > 0) parts.push(`${plural(documents, 'document', 'documents')} opened`);
  return parts.join(' · ');
}
