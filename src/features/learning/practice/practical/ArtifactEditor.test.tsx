import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { LearningPracticalActivityDto } from '@/lib/bindings';

import { ArtifactEditor } from './ArtifactEditor';

vi.mock('./LearningCodeEditor', () => ({ LearningCodeEditor: () => <div /> }));

const activity = {
  id: 'activity-1',
  revision: 1,
  runtimeKind: 'none',
  files: [
    { path: 'a.py', role: 'source', content: 'one', contentSha256: 'x', editable: true },
    { path: 'b.py', role: 'source', content: 'two', contentSha256: 'y', editable: true },
  ],
} as unknown as LearningPracticalActivityDto;

describe('ArtifactEditor', () => {
  it('marks a file edited only when it differs from the activity file at that path', () => {
    const editorFiles = [
      { path: 'a.py', content: 'one changed' },
      { path: 'b.py', content: 'two' },
    ];
    render(
      <ArtifactEditor
        activeActivity={activity}
        draft={{ loadState: 'ready', saveState: 'saved', error: null, setFileContent: vi.fn() } as never}
        editorFiles={editorFiles as never}
        selectedFile={editorFiles[0] as never}
        setSelectedFilePath={vi.fn()}
        canRunActivity={false}
        startingRun={false}
        runPending={false}
        activeRun={undefined}
        beginRun={vi.fn(async () => {})}
        builtinRuntime={undefined}
        capability={undefined}
      />,
    );
    expect(within(screen.getByRole('tab', { name: /a\.py/ })).getByText('Edited')).toBeInTheDocument();
    expect(within(screen.getByRole('tab', { name: /b\.py/ })).queryByText('Edited')).not.toBeInTheDocument();
  });
});
