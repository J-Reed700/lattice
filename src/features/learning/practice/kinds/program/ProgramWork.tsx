import { useState, type KeyboardEvent } from 'react';

import { newId } from '@/lib/api';

import { programApi } from './programApi';
import { RunBar } from './RunBar';
import { programDraftState } from './useProgramDraft';
import { programWork } from './work';
import { LearningCodeEditor } from '../../practical/LearningCodeEditor';
import type { KindPartProps } from '../../session/kind';
import { FileTabs } from '../../shared/FileTabs';

export function ProgramWork({ programId, session, draft, checking, check, own, touch }: KindPartProps) {
  const { files, setFileContent, currentFiles } = programDraftState(draft);
  const [activePath, setActivePath] = useState<string | null>(null);
  const current = files.find((file) => file.path === activePath) ?? files[0];

  const runInput = (input: string[]) => {
    if (checking) return;
    touch();
    const ids = { operationId: newId(), runId: newId() };
    const attempt = (): Promise<boolean> =>
      own(
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
    void attempt();
  };

  const onEditorKeys = (event: KeyboardEvent) => {
    if (event.key === 'Enter' && event.shiftKey && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      event.stopPropagation();
      void check('all');
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
            onRunShortcut={() => void check('shown')}
          />
        ) : null}
      </div>

      <RunBar
        signature={programWork(session).problem.signature}
        running={checking}
        onRunExamples={() => void check('shown')}
        onSubmit={() => void check('all')}
        onTryInput={runInput}
      />
    </>
  );
}
