import type { ActivityEntryDto, ActivitySnapshotDto } from '@/lib/bindings';

/**
 * Iris: the app's small light. She reads what Lattice already knows (the
 * activity snapshot, the index's failures, failed downloads, due flashcards)
 * and says it in one line. She never guesses and never calls a model: every
 * word here comes from a number the backend reported.
 */

/** How the light moves. */
export type IrisMood =
  /** Nothing running: a slow breath. */
  | 'idle'
  /** Indexing, downloading, a job: brighter, with a ring. */
  | 'working'
  /** A model is answering a request: a quick shimmer. */
  | 'thinking'
  /** Something new to tell you: one soft pulse, then a steady glow until you look. */
  | 'notice';

/** The light's colour: blue, or amber for something that failed and you haven't seen yet. */
export type IrisTone = 'calm' | 'alert';

/** Where a note's action goes. The view maps each to a route or a drawer. */
export type IrisDestination = 'downloads' | 'indexing' | 'flashcards';

export interface IrisNote {
  /** Stable across reads while the fact holds, so "seen" survives a refresh. */
  id: string;
  tone: IrisTone;
  text: string;
  action: { label: string; to: IrisDestination } | null;
}

export interface IrisInputs {
  activity: ActivitySnapshotDto | undefined;
  /** Files the current indexing run could not read. */
  indexingFailures: number;
  /** Downloads that ended in failure, by display name. */
  failedDownloads: readonly { id: string; name: string }[];
  /** Flashcards due across every deck; null while unknown. */
  dueCards: number | null;
  /** Notes already shown to you when you opened Iris. */
  seenNoteIds: ReadonlySet<string>;
}

export interface IrisReading {
  mood: IrisMood;
  tone: IrisTone;
  /** 0–1 across running work with a measured total; null when unknown. */
  progress: number | null;
  /** One sentence for the card. */
  headline: string;
  /** A few words for the rail and the button's label. */
  short: string;
  notes: IrisNote[];
  /** Running first, then paused, then waiting, as the snapshot orders them. */
  work: ActivityEntryDto[];
  /** Whether `notes` holds something you have not opened Iris on yet. */
  unseen: boolean;
}

/** A model answering, as opposed to loading weights or other work. */
export function isThinking(item: Pick<ActivityEntryDto, 'kind' | 'state'>): boolean {
  return item.state === 'running' && item.kind === 'model.requests';
}

function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** The mean fraction done across running items that report a total. */
export function aggregateProgress(items: readonly ActivityEntryDto[]): number | null {
  const fractions = items
    .filter((item) => item.state === 'running' && item.progress !== null && item.progress.total > 0)
    .map((item) => Math.min(1, Math.max(0, item.progress!.done / item.progress!.total)));
  if (fractions.length === 0) return null;
  return fractions.reduce((sum, value) => sum + value, 0) / fractions.length;
}

/** The facts worth a word, alerts first. */
export function irisNotes(inputs: Pick<IrisInputs, 'indexingFailures' | 'failedDownloads' | 'dueCards'>): IrisNote[] {
  const notes: IrisNote[] = [];
  for (const download of inputs.failedDownloads) {
    notes.push({
      id: `download-failed:${download.id}`,
      tone: 'alert',
      text: `The ${download.name} download failed.`,
      action: { label: 'Open downloads', to: 'downloads' },
    });
  }
  if (inputs.indexingFailures > 0) {
    notes.push({
      // The count is in the id: a new failure is a new fact.
      id: `index-failed:${inputs.indexingFailures}`,
      tone: 'alert',
      text: `${plural(inputs.indexingFailures, 'file', 'files')} couldn't be indexed.`,
      action: { label: 'See why', to: 'indexing' },
    });
  }
  if (inputs.dueCards !== null && inputs.dueCards > 0) {
    notes.push({
      // Not the count: cards keep coming due through the day, and that is
      // not news each time.
      id: 'flashcards-due',
      tone: 'calm',
      text: `${plural(inputs.dueCards, 'flashcard is', 'flashcards are')} due for review.`,
      action: { label: 'Review', to: 'flashcards' },
    });
  }
  return notes;
}

function percent(progress: number): string {
  return `${Math.round(progress * 100)}%`;
}

function workHeadline(running: readonly ActivityEntryDto[], progress: number | null): { headline: string; short: string } {
  if (running.length === 1) {
    const [only] = running;
    const thinking = isThinking(only);
    const title = thinking ? `${only.title} is answering` : only.title;
    return {
      headline: progress === null ? `${title}.` : `${title}, ${percent(progress)} done.`,
      short: thinking ? 'Thinking' : progress === null ? 'Working' : `Working · ${percent(progress)}`,
    };
  }
  const anyThinking = running.some(isThinking);
  return {
    headline: `Working on ${running.length} things${progress === null ? '' : `, ${percent(progress)} done`}.`,
    short: anyThinking ? 'Thinking' : `${running.length} running`,
  };
}

/** What Iris shows for the app's state right now. */
export function readIris(inputs: IrisInputs): IrisReading {
  const work = inputs.activity?.items ?? [];
  const running = work.filter((item) => item.state === 'running');
  const notes = irisNotes(inputs);
  // A note going away is not news; only one you haven't seen is.
  const fresh = notes.filter((note) => !inputs.seenNoteIds.has(note.id));
  const unseen = fresh.length > 0;
  const progress = aggregateProgress(work);
  const tone: IrisTone = fresh.some((note) => note.tone === 'alert') ? 'alert' : 'calm';

  let mood: IrisMood;
  let headline: string;
  let short: string;
  if (running.length > 0) {
    mood = running.some(isThinking) ? 'thinking' : 'working';
    ({ headline, short } = workHeadline(running, progress));
  } else if (unseen) {
    mood = 'notice';
    headline = fresh[0].text;
    short = fresh.length === 1 ? 'One new thing' : `${fresh.length} new things`;
  } else if (work.length > 0) {
    mood = 'idle';
    headline = `${plural(work.length, 'task is', 'tasks are')} paused or waiting.`;
    short = 'Waiting';
  } else {
    mood = 'idle';
    headline = notes.length > 0 ? 'Nothing is running.' : 'All quiet. Nothing is running.';
    short = 'Quiet';
  }

  return { mood, tone, progress: running.length > 0 ? progress : null, headline, short, notes, work, unseen };
}

/** The button's accessible name: "Iris: All quiet. Nothing is running.", plus how many notes the card holds. */
export function irisLabel(reading: Pick<IrisReading, 'headline' | 'notes'>): string {
  const extra = reading.notes.length > 0 ? ` ${plural(reading.notes.length, 'thing', 'things')} to look at.` : '';
  return `Iris: ${reading.headline}${extra}`;
}
