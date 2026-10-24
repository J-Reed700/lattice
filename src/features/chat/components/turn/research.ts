import { runsOf } from '@/features/chat/components/turn/rounds';
import type { TurnStep } from '@/types/conversation';

export type PageState =
  | 'found'
  | 'reading'
  | 'read'
  | 'failed';

export interface DeckPage {
  url: string;
  host: string;
  title: string | null;
  state: PageState;
  note: string | null;
  picture?: boolean;
}

export interface DeckQuery {
  id: string;
  text: string;
  running: boolean;
  failure: string | null;
}

export interface DeckRound {
  type: 'round';
  round: number;
  queries: DeckQuery[];
  pages: DeckPage[];
  otherSteps: TurnStep[];
  otherStepsCameFirst: boolean;
  active: boolean;
}

export interface DeckThinking {
  type: 'thinking';
  step: TurnStep;
  wentBack: boolean;
  wentBackTo: DeckRound | null;
}

export interface DeckStep {
  type: 'step';
  step: TurnStep;
}

export type DeckItem = DeckRound | DeckThinking | DeckStep;

export interface Deck {
  items: DeckItem[];
  deepResearch: boolean;
  modeNote: string | null;
  rounds: number;
}

const WEB_KINDS = new Set<TurnStep['kind']>(['web_search', 'read_page', 'wiki', 'find_image']);

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

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
    if (step.kind === 'find_image') {
      for (const link of step.links ?? []) {
        pages.set(pageKey(link.url), {
          url: link.url,
          host: hostOf(link.url),
          title: link.title ?? null,
          state: step.state === 'running' ? 'reading' : step.state === 'done' ? 'read' : 'failed',
          note: null,
          picture: true,
        });
      }
      if ((step.links?.length ?? 0) === 0) otherSteps.push(step);
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
      if (step.kind === 'deep_research') continue;
      items.push(
        step.kind === 'generate'
          ? { type: 'thinking', step, wentBack: false, wentBackTo: null }
          : { type: 'step', step }
      );
    }
  }

  items.forEach((item, index) => {
    if (item.type !== 'thinking') return;
    const next = items.slice(index + 1).find(later => later.type !== 'step');
    item.wentBack = next?.type === 'round' && hadRoundBefore(items, index);
    item.wentBackTo = item.wentBack && next?.type === 'round' ? next : null;
  });

  return {
    items,
    deepResearch: mode !== undefined,
    modeNote: mode?.result ?? null,
    rounds,
  };
}

const RETURN_QUERY_CHARS = 80;

export function returnLine(round: DeckRound): string {
  const [first, ...rest] = round.queries;
  if (first) {
    const text =
      first.text.length > RETURN_QUERY_CHARS ? `${first.text.slice(0, RETURN_QUERY_CHARS - 1)}…` : first.text;
    const more = rest.length > 0 ? ` and ${plural(rest.length, 'more search', 'more searches')}` : '';
    return `Not enough yet — searching again for “${text}”${more}`;
  }
  const opened = round.pages.filter(page => page.state !== 'found').length;
  if (opened > 0) return `Not enough yet — reading ${plural(opened, 'more page', 'more pages')}`;
  return 'Not enough yet — going back for another look';
}

function hadRoundBefore(items: readonly DeckItem[], index: number): boolean {
  return items.slice(0, index).some(item => item.type === 'round');
}

const plural = (count: number, one: string, many: string) =>
  `${count.toLocaleString()} ${count === 1 ? one : many}`;

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

export function hostHue(host: string): number {
  let hash = 0;
  for (const char of host) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) % 360;
  return hash;
}
