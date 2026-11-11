import type { ComponentType, ReactNode } from 'react';

import type * as Wire from '@/lib/bindings';

export type Failure = { message: string; retry?: () => void };

/** The learner's draft as the kind holds it. `state` is the kind's own; only its components read it. */
export interface KindDraft {
  /** The work as it stands now, as the wire carries it. */
  piece(): Wire.JsonValue;
  /** Changes exactly when the piece changes. */
  signature: string;
  /** Saves what is unsaved. */
  flush(): Promise<void>;
  /** A save that failed, with its retry. */
  failure: Failure | null;
  state: unknown;
}

/** What every part of a kind's view is given. */
export interface KindPartProps {
  programId: string;
  session: Wire.PracticeSessionDto;
  draft: KindDraft;
  /** A check, or one of the kind's own commands, is running. */
  checking: boolean;
  /** The engine's check of the piece as it stands. */
  check(scope: 'shown' | 'all'): Promise<void>;
  /** Runs one of the kind's own commands and applies the session it answers with. False when it failed; the frame shows the failure. */
  own(call: () => Promise<Wire.PracticeSessionDto>, retry?: () => void): Promise<boolean>;
  /** The learner did something: the idle timer starts again. */
  touch(): void;
}

/** The task's part also receives the frame's button for opening the worked solution, to place where the solution belongs. */
export interface KindTaskProps extends KindPartProps {
  solutionControl: ReactNode;
}

/** Display text that is the kind's. */
export interface KindCopy {
  /** The names of the two panels the kind fills, as the small-screen tabs show them. */
  taskPanel: string;
  workPanel: string;
  /** Shown while the counterpart reads the work. */
  readingWork: string;
  stageInstruction(stage: Wire.Stage): string;
  rung(rung: Wire.Rung): { name: string; ask: string };
}

export interface KindView {
  copy: KindCopy;
  /** What the kind reads once before the session opens. */
  prepare(programId: string, session: Wire.PracticeSessionDto): Promise<unknown>;
  useDraft(args: { programId: string; session: Wire.PracticeSessionDto; prepared: unknown }): KindDraft;
  /** The task and what belongs beside it: the reference once it is open, the learner's own cases. */
  Task: ComponentType<KindTaskProps>;
  /** The editor and the controls that check the work. */
  Work: ComponentType<KindPartProps>;
  /** What the checks found. */
  Results: ComponentType<KindPartProps>;
  /** Where a hint points, when the kind's hints point at parts of the work. */
  HintMark?: ComponentType<{ session: Wire.PracticeSessionDto; hint: Wire.PracticeHintDto }>;
  /** The kind's part of the debrief. */
  Debrief: ComponentType<{ session: Wire.PracticeSessionDto; debrief: Wire.PracticeDebriefDto }>;
}

export type Kinds = Readonly<Record<string, KindView>>;
