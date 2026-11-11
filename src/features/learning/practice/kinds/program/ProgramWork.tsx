import { useState, type KeyboardEvent } from 'react';

import { newId } from '@/lib/api';

import { programApi } from './programApi';
import { RunBar } from './RunBar';
import { programDraftState } from './useProgramDraft';
import { programWork } from './work';
import { LearningCodeEditor } from '../../practical/LearningCodeEditor';
import { FileTabs } from '../../shared/FileTabs';

import type { KindPartProps } from '../../session/kind';

export function ProgramWork({ programId, session, draft, checking, check, own, touch }: KindPartProps) {
  const { files, setFileContent, currentFiles } = programDraftState(draft);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [runningInput, setRunningInput] = useState(false);
  const current = files.find((file) => file.path === activePath) ?? files[0];

  const running = checking || runningInput;

  const run = (scope: 'shown' | 'all') => {
    if (runningInput) return;
    void check(scope);
  };

  const runInput = (input: string[]) => {
    if (running) return;
    touch();
    const ids = { operationId: newId(), runId: newId() };
    const attempt = async (): Promise<boolean> => {
      setRunningInput(true);
      try {
        return await own(
          () =>
            programApi.runInput({
              ...ids,
              programId,
              sessionId: session.sessionId,
              files: currentFiles(),
              input,
            }),
          () => void attempt(),
        );
      } finally {
        setRunningInput(false);
      }
    };
    void attempt();
  };

  const onEditorKeys = (event: KeyboardEvent) => {
    if (event.key === 'Enter' && event.shiftKey && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      event.stopPropagation();
      run('all');
    }
  };

  return (
    <>
      <div className="overflow-hidden rounded-xl border border-border-default bg-surface" onKeyDownCapture={onEditorKeys}>
        {files.length > 1 ? (
          <FileTabs
            label="Files"
            files={files.map((file) => ({ path: file.path }))}
            active={current?.path ?? ''}
            onSelect={setActivePath}
          />
        ) : null}
        {current ? (
          <LearningCodeEditor
            path={current.path}
            value={current.content}
            onChange={(value) => {
              touch();
              setFileContent(current.path, value);
            }}
            ariaLabel="Code"
            minHeight={280}
            showPath={files.length <= 1}
            onRunShortcut={() => run('shown')}
          />
        ) : null}
      </div>

      <RunBar
        signature={programWork(session).problem.signature}
        running={running}
        onRunExamples={() => run('shown')}
        onSubmit={() => run('all')}
        onTryInput={runInput}
      />
    </>
  );
}
