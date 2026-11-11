import { useRef, useState } from 'react';

import { vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import type { KindDraft, KindPartProps, KindTaskProps, KindView } from '../kind';

interface FakeState {
  text: string;
  setText: (text: string) => void;
}

function fakeWork(session: Wire.PracticeSessionDto): { brief: string; start: string } {
  return session.work as unknown as { brief: string; start: string };
}

/** What the draft's flush calls, so a test can see the work was saved first. */
export const flushDraft = vi.fn(async () => undefined);

function useFakeDraft({ session }: { programId: string; session: Wire.PracticeSessionDto }): KindDraft {
  const [text, setText] = useState(fakeWork(session).start);
  const ref = useRef(text);
  ref.current = text;
  return {
    piece: () => ref.current,
    signature: text,
    flush: () => flushDraft(),
    failure: null,
    state: { text, setText } satisfies FakeState,
  };
}

function FakeTask({ session, solutionControl }: KindTaskProps) {
  return (
    <section aria-label="Brief">
      <p>{fakeWork(session).brief}</p>
      {solutionControl}
    </section>
  );
}

/** A kind whose piece is a string held in component state; its own command is whatever the test supplies. */
export function createFakeKind(ownCommand: () => Promise<Wire.PracticeSessionDto>): KindView {
  function FakeWork({ draft, checking, check, own, touch }: KindPartProps) {
    const { text, setText } = draft.state as FakeState;
    const again = (): Promise<boolean> => own(ownCommand, () => void again());
    return (
      <div>
        <textarea
          aria-label="Work"
          value={text}
          onChange={(event) => {
            touch();
            setText(event.target.value);
          }}
        />
        <button type="button" disabled={checking} onClick={() => void check('shown')}>
          Check shown
        </button>
        <button type="button" disabled={checking} onClick={() => void check('all')}>
          Check all
        </button>
        <button type="button" onClick={() => void again()}>
          Do the kind&apos;s own thing
        </button>
      </div>
    );
  }

  function FakeResults({ session }: KindPartProps) {
    return (
      <ul aria-label="Checks">
        {session.checks.map((item) => (
          <li key={item.checkId}>{`${item.scope} ${item.passed}/${item.total}`}</li>
        ))}
      </ul>
    );
  }

  return {
    copy: {
      taskPanel: 'Brief',
      workPanel: 'Piece',
      readingWork: 'Reading the work…',
      stageInstruction: (stage) => `Do the ${stage} stage.`,
      rung: (rung) =>
        ({
          nudge: { name: 'Nudge', ask: 'Get a nudge', counted: 'small push' },
          idea: { name: 'Idea', ask: 'Get the idea', counted: 'notion' },
          plan: { name: 'Plan', ask: 'Get a plan', counted: 'outline' },
          point: { name: 'Point', ask: 'Point at the part', counted: 'pointer' },
        })[rung],
      debrief: {
        check: 'trial',
        learnerCase: 'Case',
        techniques: 'Techniques',
        caught: 'Caught',
        missed: 'Missed',
      },
    },
    prepare: async () => 'prepared',
    useDraft: useFakeDraft,
    Task: FakeTask,
    Work: FakeWork,
    Results: FakeResults,
    HintMark: ({ hint }) => <p>{`Points at part ${hint.ordinal}`}</p>,
    Debrief: ({ debrief }) => <p>{(debrief.work as { note: string }).note}</p>,
  };
}
