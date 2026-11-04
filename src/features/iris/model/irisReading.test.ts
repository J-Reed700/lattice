import { describe, expect, it } from 'vitest';

import type { ActivityEntryDto } from '@/lib/bindings';

import { aggregateProgress, irisAnnouncement, irisLabel, irisNotes, readIris, type IrisInputs } from './irisReading';

function entry(overrides: Partial<ActivityEntryDto> = {}): ActivityEntryDto {
  return {
    id: 'a',
    kind: 'download',
    title: 'Downloading Qwen',
    state: 'running',
    detail: null,
    progress: null,
    perSecond: null,
    secondsLeft: null,
    holding: null,
    target: null,
    since: null,
    retryAt: null,
    ...overrides,
  };
}

function inputs(overrides: Partial<IrisInputs> = {}): IrisInputs {
  return {
    activity: { items: [], running: 0 },
    activityFailed: false,
    indexingFailures: 0,
    failedDownloads: [],
    dueCards: 0,
    seenNoteIds: new Set(),
    ...overrides,
  };
}

describe('readIris', () => {
  it('is quiet when nothing runs and nothing needs a look', () => {
    const reading = readIris(inputs());
    expect(reading).toMatchObject({ mood: 'idle', tone: 'calm', progress: null, short: 'Quiet', unseen: false });
    expect(reading.headline).toBe('All quiet. Nothing is running.');
  });

  it('works with a ring and says how far one task has got', () => {
    const reading = readIris(inputs({
      activity: { items: [entry({ progress: { done: 42, total: 100, unit: 'bytes' } })], running: 1 },
    }));
    expect(reading).toMatchObject({ mood: 'working', progress: 0.42, short: 'Working · 42%' });
    expect(reading.headline).toBe('Downloading Qwen, 42% done.');
  });

  it('thinks while a model answers, whatever else runs', () => {
    const reading = readIris(inputs({
      activity: {
        items: [entry(), entry({ id: 'b', kind: 'model.requests', title: 'Qwen3-8B' })],
        running: 2,
      },
    }));
    expect(reading.mood).toBe('thinking');
    expect(reading.short).toBe('Thinking');
    expect(reading.headline).toBe('Working on 2 things.');
  });

  it('names the model when answering is all it does', () => {
    const reading = readIris(inputs({
      activity: { items: [entry({ kind: 'model.requests', title: 'Qwen3-8B' })], running: 1 },
    }));
    expect(reading.headline).toBe('Qwen3-8B is answering.');
  });

  it('does not count a model loading its weights as thinking', () => {
    const reading = readIris(inputs({
      activity: { items: [entry({ kind: 'model.loading', title: 'Loading Qwen3-8B' })], running: 1 },
    }));
    expect(reading.mood).toBe('working');
  });

  it('notices something new, then settles once it has been seen', () => {
    const fresh = readIris(inputs({ dueCards: 12 }));
    expect(fresh).toMatchObject({ mood: 'notice', tone: 'calm', unseen: true, short: 'One new thing' });
    expect(fresh.headline).toBe('12 flashcards are due for review.');

    const seen = readIris(inputs({ dueCards: 12, seenNoteIds: new Set(['flashcards-due']) }));
    expect(seen).toMatchObject({ mood: 'idle', unseen: false, headline: 'Nothing is running.' });
    expect(seen.notes).toHaveLength(1);
  });

  it('does not treat more cards coming due as news', () => {
    const reading = readIris(inputs({ dueCards: 13, seenNoteIds: new Set(['flashcards-due']) }));
    expect(reading.unseen).toBe(false);
  });

  it('does not treat a note going away as news', () => {
    const reading = readIris(inputs({
      indexingFailures: 2,
      seenNoteIds: new Set(['index-failed:2', 'flashcards-due']),
    }));
    expect(reading.unseen).toBe(false);
  });

  it('turns amber for a failure it has not shown, and blue again once seen', () => {
    const failing = readIris(inputs({ failedDownloads: [{ id: 'd1', name: 'Qwen3-8B' }] }));
    expect(failing).toMatchObject({ mood: 'notice', tone: 'alert', headline: 'The Qwen3-8B download failed.' });

    const seen = readIris(inputs({ failedDownloads: [{ id: 'd1', name: 'Qwen3-8B' }], seenNoteIds: new Set(['download-failed:d1']) }));
    expect(seen.tone).toBe('calm');
  });

  it('stays amber while working if a failure is unseen', () => {
    const reading = readIris(inputs({
      activity: { items: [entry()], running: 1 },
      indexingFailures: 1,
    }));
    expect(reading).toMatchObject({ mood: 'working', tone: 'alert' });
  });

  it('says when work is only paused or waiting', () => {
    const reading = readIris(inputs({ activity: { items: [entry({ state: 'waiting' }), entry({ id: 'b', state: 'paused' })], running: 0 } }));
    expect(reading).toMatchObject({ mood: 'idle', short: 'Waiting', headline: '2 tasks are paused or waiting.' });
  });

  it('reads an unknown snapshot as nothing running', () => {
    expect(readIris(inputs({ activity: undefined })).mood).toBe('idle');
  });

  it('says so when it could not read what is running, and never calls that quiet', () => {
    const failing = readIris(inputs({ activity: undefined, activityFailed: true }));
    expect(failing).toMatchObject({ mood: 'notice', tone: 'alert', unseen: true, activityFailed: true, short: 'One new thing' });
    expect(failing.headline).toBe('Could not read what is running.');

    const seen = readIris(inputs({ activity: undefined, activityFailed: true, seenNoteIds: new Set(['activity-unreadable']) }));
    expect(seen).toMatchObject({ mood: 'idle', tone: 'calm', unseen: false, short: "Can't tell" });
    expect(seen.headline).toBe('Could not read what is running.');
  });

  it('keeps the work it last read when a later read fails', () => {
    const reading = readIris(inputs({ activity: { items: [entry()], running: 1 }, activityFailed: true }));
    expect(reading).toMatchObject({ mood: 'working', tone: 'alert', activityFailed: true });
    expect(reading.work).toHaveLength(1);
    expect(reading.notes.map((note) => note.id)).toEqual(['activity-unreadable']);
  });
});

describe('irisNotes', () => {
  it('lists failures before news, each with somewhere to go', () => {
    const notes = irisNotes({ activityFailed: false, indexingFailures: 1, failedDownloads: [{ id: 'd', name: 'MiniLM' }], dueCards: 1 });
    expect(notes.map((note) => [note.text, note.action?.to])).toEqual([
      ['The MiniLM download failed.', 'downloads'],
      ['1 file couldn\'t be indexed.', 'indexing'],
      ['1 flashcard is due for review.', 'flashcards'],
    ]);
  });

  it('puts a failed read of what is running first, with nowhere to go', () => {
    const notes = irisNotes({ activityFailed: true, indexingFailures: 1, failedDownloads: [], dueCards: 1 });
    expect(notes.map((note) => [note.id, note.tone, note.action?.to ?? null])).toEqual([
      ['activity-unreadable', 'alert', null],
      ['index-failed:1', 'alert', 'indexing'],
      ['flashcards-due', 'calm', 'flashcards'],
    ]);
  });

  it('says nothing about flashcards it could not count', () => {
    expect(irisNotes({ activityFailed: false, indexingFailures: 0, failedDownloads: [], dueCards: null })).toEqual([]);
  });
});

describe('aggregateProgress', () => {
  it('averages what running items measured and skips the rest', () => {
    expect(aggregateProgress([
      entry({ progress: { done: 1, total: 4, unit: 'files' } }),
      entry({ id: 'b', progress: { done: 3, total: 4, unit: 'bytes' } }),
      entry({ id: 'c' }),
      entry({ id: 'd', state: 'paused', progress: { done: 0, total: 4, unit: 'files' } }),
    ])).toBe(0.5);
  });

  it('is unknown when nothing reports a total', () => {
    expect(aggregateProgress([entry({ progress: { done: 3, total: 0, unit: 'files' } })])).toBeNull();
  });
});

describe('irisAnnouncement', () => {
  it('counts what runs, and keeps to counts so progress does not chatter', () => {
    expect(irisAnnouncement([entry(), entry({ id: 'b' }), entry({ id: 'c', state: 'paused' })])).toBe('Iris: 2 running');
    expect(irisAnnouncement([entry({ state: 'waiting' }), entry({ id: 'b', state: 'paused' })])).toBe('Iris: 2 waiting or paused');
    expect(irisAnnouncement([])).toBe('Iris: nothing running');

    const half = entry({ progress: { done: 50, total: 100, unit: 'bytes' } });
    const most = entry({ progress: { done: 90, total: 100, unit: 'bytes' } });
    expect(irisAnnouncement([half])).toBe(irisAnnouncement([most]));
  });
});

describe('irisLabel', () => {
  it('counts what is new for a screen reader', () => {
    expect(irisLabel(readIris(inputs({ dueCards: 2, indexingFailures: 1 })))).toBe(
      'Iris: 1 file couldn\'t be indexed. 2 things to look at.',
    );
    expect(irisLabel(readIris(inputs()))).toBe('Iris: All quiet. Nothing is running.');
  });
});
