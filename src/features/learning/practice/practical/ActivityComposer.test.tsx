import { createRef } from 'react';

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

import { ActivityComposer } from './ActivityComposer';

const program = { summary: { id: 'course-1', revision: 1 } } as LearningProgramDto;
const lesson = { id: 'lesson-1' } as LearningLessonDto;

function show(props: Partial<React.ComponentProps<typeof ActivityComposer>> = {}) {
  render(
    <ActivityComposer
      program={program}
      lesson={lesson}
      profiles={[]}
      builtinRuntimes={[]}
      returnFocusRef={createRef()}
      onClose={vi.fn()}
      onCreated={vi.fn()}
      onRequestRuntimeSetup={vi.fn()}
      {...props}
    />,
  );
}

const offered = () => screen.getAllByRole('option').map((option) => (option as HTMLOptionElement).value);

describe('ActivityComposer', () => {
  it('no longer offers code review, debugging or project', () => {
    show();
    const kinds = offered();
    for (const gone of ['code_review', 'debugging', 'project']) expect(kinds).not.toContain(gone);
    expect(kinds).toEqual(expect.arrayContaining(['code_lab', 'incident', 'system_design', 'interview', 'conversation']));
  });

  it('falls back to a code lab when asked for a kind it no longer offers', () => {
    show({ initialKind: 'project' });
    expect(screen.getByLabelText('Activity type')).toHaveValue('code_lab');
  });

  it('opens already set to an interview with the company, role and round it was given', () => {
    show({ initialKind: 'interview', initialInterview: { companyName: 'Acme', targetRole: 'Engineer', roundKind: 'system_design' } });
    expect(screen.getByLabelText('Activity type')).toHaveValue('interview');
    expect(screen.getByLabelText('Company name', { exact: true })).toHaveValue('Acme');
    expect(screen.getByLabelText('Target role')).toHaveValue('Engineer');
    expect(screen.getByLabelText('Interview round')).toHaveValue('system_design');
  });
});
