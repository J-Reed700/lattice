import type { TurnStep } from '@/types/conversation';

import { runsOf } from './rounds';

/**
 * A turn's trips to the web, as something to watch rather than a log to read.
 *
 * A research turn runs for minutes and most of that is searching and reading.
 * In the flat timeline the second and third trips are a few more grey rows; a
 * reader cannot see *where* the model went, which pages refused it, or that it
 * chose to go back. This groups the same steps the timeline has — nothing here
 * is a second source of truth — into what a person asks of a long turn: what
 * did it search for, which pages came back, which did it actually read.
 */

export type PageState =
  /** A search listed it; nothing has tried to read it. */
  | 'found'
  | 'reading'
  | 'read'
  /** A read was attempted and refused, timed out or came back empty. */
  | 'failed';

export interface DeckPage {
  url: string;
  host: string;
  title: string | null;
  state: PageState;
  /** "1,204 words", or the reason a read failed. */
  note: string | null;
}

export interface DeckQuery {
  id: string;
  text: string;
  running: boolean;
  /** Why it failed, when it did. A search that died silently looks like one that found nothing. */
  failure: string | null;
}

export interface DeckRound {
  type: 'round';
  /** 1-based among every gathering run of the turn, web or not. */
  round: number;
  queries: DeckQuery[];
  pages: DeckPage[];
  /** Gathering in this run that went nowhere on the web: documents, tools. */
  otherSteps: TurnStep[];
  /**
   * Those steps began before the web did. A turn searches its own documents
   * first; drawing that under the pages would tell the story backwards.
   */
  otherStepsCameFirst: boolean;
  /** Something in the round is still running. */
  active: boolean;
}

export interface DeckThinking {
  type: 'thinking';
  step: TurnStep;
  /** This generation was followed by another trip, so it chose to go back. */
  wentBack: boolean;
}

export interface DeckStep {
  type: 'step';
  step: TurnStep;
}

export type DeckItem = DeckRound | DeckThinking | DeckStep;

export interface Deck {
  items: DeckItem[];
  /** The turn was asked to research, not just to answer. */
  deepResearch: boolean;
  /** What that mode does, in the backend's words, for the deck's heading. */
  modeNote: string | null;
  rounds: number;
}

const WEB_KINDS = new Set<TurnStep['kind']>(['web_search', 'read_page', 'wiki']);

/** `www.` says nothing, and the host is read at a glance. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

/**
 * One address, however it was written. A search lists `https://a.com/x/` and
 * the model asks for `https://a.com/x` — the same page, and filing them apart
 * would leave the listed one looking unread beside a stray duplicate.
 */
function pageKey(url: string): string {
  try {
    const parsed = new URL(url);
    parsed.hash = '';
    const path = parsed.pathname.replace(/\/+$/, '');
    return `${parsed.hostname.replace(/^www\./, '')}${path}${parsed.search}`.toLowerCase();
  } catch {
    return url.trim().toLowerCase();
  }
}

/** Whether there is anything here worth more than the flat timeline. */
export function hasWebResearch(steps: readonly TurnStep[]): boolean {
  return steps.some(step => WEB_KINDS.has(step.kind) && (step.links?.length ?? 0) > 0);
}

function buildRound(round: number, steps: TurnStep[]): DeckRound {
  const pages = new Map<string, DeckPage>();
  const queries: DeckQuery[] = [];
  const otherSteps: TurnStep[] = [];

  for (const step of steps) {
    if (step.kind === 'read_page') {
      const link = step.links?.[0];
      if (!link) {
        otherSteps.push(step);
        continue;
      }
      const key = pageKey(link.url);
      const listed = pages.get(key);
      pages.set(key, {
        url: link.url,
        host: hostOf(link.url),
        title: link.title ?? listed?.title ?? null,
        state: step.state === 'running' ? 'reading' : step.state === 'done' ? 'read' : 'failed',
        note: step.result ?? null,
      });
      continue;
    }
    if (step.kind === 'web_search' || step.kind === 'wiki') {
      if (step.detail) {
        queries.push({
          id: step.id,
          text: step.detail,
          running: step.state === 'running',
          failure: step.state === 'failed' ? (step.result ?? 'failed') : null,
        });
      }
      for (const link of step.links ?? []) {
        const key = pageKey(link.url);
        // A page already being read keeps its state; a second search listing
        // it again must not knock it back to "found".
        if (pages.has(key)) continue;
        pages.set(key, {
          url: link.url,
          host: hostOf(link.url),
          title: link.title ?? null,
          state: 'found',
          note: null,
        });
      }
      if (!step.detail && (step.links?.length ?? 0) === 0) otherSteps.push(step);
      continue;
    }
    otherSteps.push(step);
  }

  const firstWeb = steps.find(step => !otherSteps.includes(step));
  const firstOther = otherSteps[0];
  return {
    type: 'round',
    round,
    otherStepsCameFirst:
      firstWeb !== undefined && firstOther !== undefined && steps.indexOf(firstOther) < steps.indexOf(firstWeb),
    queries,
    pages: [...pages.values()],
    otherSteps,
    active: steps.some(step => step.state === 'running'),
  };
}

export function deckOf(steps: readonly TurnStep[]): Deck {
  const items: DeckItem[] = [];
  const mode = steps.find(step => step.kind === 'deep_research');
  let rounds = 0;
  for (const run of runsOf(steps)) {
    if (run.gathering) {
      rounds += 1;
      items.push(buildRound(rounds, run.steps));
      continue;
    }
    for (const step of run.steps) {
      // Said by the deck's own heading, not as a row of work that was done.
      if (step.kind === 'deep_research') continue;
      items.push(
        step.kind === 'generate'
          ? { type: 'thinking', step, wentBack: false }
          : { type: 'step', step }
      );
    }
  }

  // A generation "went back" when another trip follows it before the next
  // generation. Decided here, from the list, because the step itself only
  // knows it asked for tools — not whether anything came of asking.
  items.forEach((item, index) => {
    if (item.type !== 'thinking') return;
    const next = items.slice(index + 1).find(later => later.type !== 'step');
    item.wentBack = next?.type === 'round' && hadRoundBefore(items, index);
  });

  return {
    items,
    deepResearch: mode !== undefined,
    modeNote: mode?.result ?? null,
    rounds,
  };
}

function hadRoundBefore(items: readonly DeckItem[], index: number): boolean {
  return items.slice(0, index).some(item => item.type === 'round');
}

const plural = (count: number, one: string, many: string) =>
  `${count.toLocaleString()} ${count === 1 ? one : many}`;

/**
 * A round in one line. "4 of 6 pages read" rather than "4 pages read": the two
 * that refused are usually why the model went back, and hiding them makes the
 * next round look like indecision.
 */
export function roundLine(round: DeckRound): string {
  const parts: string[] = [];
  if (round.queries.length > 0) parts.push(plural(round.queries.length, 'search', 'searches'));
  const attempted = round.pages.filter(page => page.state !== 'found');
  const read = attempted.filter(page => page.state === 'read').length;
  const reading = attempted.filter(page => page.state === 'reading').length;
  if (attempted.length > 0) {
    parts.push(
      read === attempted.length
        ? `${plural(read, 'page', 'pages')} read`
        : `${read} of ${plural(attempted.length, 'page', 'pages')} read`
    );
  } else if (round.pages.length > 0) {
    parts.push(`${plural(round.pages.length, 'page', 'pages')} found`);
  }
  if (reading > 0) parts.push(`reading ${reading}`);
  return parts.join(' · ');
}

/** A stable hue per host, so a site is recognisable across rounds. */
export function hostHue(host: string): number {
  let hash = 0;
  for (const char of host) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 360;
  return hash;
}
