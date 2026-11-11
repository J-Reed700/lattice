import { useState } from 'react';

import type * as Wire from '@/lib/bindings';

import { DebriefFrame } from './DebriefFrame';
import { HintLadder } from './HintLadder';
import type { KindPartProps, KindView } from './kind';
import { SessionHeader } from './SessionHeader';
import { SolutionControl } from './SolutionControl';
import { StageRail } from './StageRail';
import { TalkPanel } from './TalkPanel';
import { useSession } from './useSession';
import { lastChosen } from '../home/StartSentence';
import { StatusLine } from '../shared/StatusLine';

export interface PracticeWorkspaceProps {
  programId: string;
  initial: Wire.PracticeSessionDto;
  prepared: unknown;
  kind: KindView;
  onExit: () => void;
  onTryInterview: (setting: Wire.Setting) => void;
}

type Panel = 'task' | 'work' | 'talk';

/** The interview setting the course last used on the Practice page; Mock the first time. */
function lastInterviewSetting(programId: string): Wire.Setting {
  return lastChosen(programId, 'mock', 'setting') === 'watched' ? 'watched' : 'mock';
}

export function PracticeWorkspace({ programId, initial, prepared, kind, onExit, onTryInterview }: PracticeWorkspaceProps) {
  const [panel, setPanel] = useState<Panel>('work');
  const draft = kind.useDraft({ programId, session: initial, prepared });
  const run = useSession({ programId, initial, draft });
  const { session } = run;
  const { Task, Work, Results, HintMark, copy } = kind;

  if (session.finished && session.debrief) {
    return (
      <DebriefFrame
        session={session}
        debrief={session.debrief}
        kind={kind}
        onExit={onExit}
        onTryInterview={() => onTryInterview(lastInterviewSetting(programId))}
      />
    );
  }

  const { setting } = session;
  const counterpart = setting === 'coached' ? 'Coach' : 'Interviewer';
  const part: KindPartProps = {
    programId,
    session,
    draft,
    checking: run.checking,
    check: run.check,
    own: run.own,
    touch: run.touch,
  };

  const panelClass = (name: Panel) =>
    `${panel === name ? 'flex' : 'hidden'} min-h-0 flex-col gap-4 lg:flex lg:overflow-y-auto`;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 p-3 sm:p-4">
      <SessionHeader
        session={session}
        clock={run.clock}
        elapsedMs={run.elapsedMs}
        finishing={run.finishing}
        onExit={onExit}
        onChangeSetting={(next) => void run.changeSetting(next)}
        onFinish={() => void run.finish()}
      />

      <StageRail
        stages={session.stages}
        current={session.stage}
        instruction={copy.stageInstruction(session.stage)}
        busy={run.moving}
        onSelect={(stage) => void run.move(stage)}
      />

      {run.status ? <StatusLine state={run.status} /> : null}

      <div role="tablist" aria-label="Workspace panels" className="flex gap-2 lg:hidden">
        {(
          [
            ['task', copy.taskPanel],
            ['work', copy.workPanel],
            ['talk', counterpart],
          ] as [Panel, string][]
        ).map(([name, label]) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={panel === name}
            onClick={() => setPanel(name)}
            className={`min-h-[44px] cursor-pointer rounded-lg px-3 text-sm ${
              panel === name ? 'bg-accent font-semibold text-accent-fg' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_minmax(0,1fr)]">
        <div className={panelClass('task')}>
          <Task {...part} solutionControl={<SolutionControl session={session} onReveal={() => void run.reveal()} />} />
        </div>

        <div className={panelClass('work')}>
          <Work {...part} />
          <Results {...part} />
          {setting !== 'watched' ? (
            <HintLadder session={session} copy={copy} HintMark={HintMark} asking={run.asking} onAsk={() => void run.ask()} />
          ) : null}
        </div>

        <div className={panelClass('talk')}>
          <TalkPanel
            session={session}
            counterpart={counterpart}
            busy={run.replyBusy ? copy.readingWork : null}
            error={run.threadError}
            asking={run.asking}
            onSend={run.send}
            onAsk={() => void run.ask()}
          />
        </div>
      </div>
    </div>
  );
}
