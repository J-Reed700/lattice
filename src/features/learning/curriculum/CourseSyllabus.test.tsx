import type { ComponentProps } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import type { LearningOutlineEvidenceDto, LearningProgramDto } from '@/lib/bindings';

const program = {
  summary: { id: 'program', title: 'Evidence course', status: 'draft' },
  modules: [{
    id: 'module', title: 'Foundations', summary: 'A sourced summary.', outcomes: ['Trace a claim.'], prerequisiteModuleIds: [], project: null,
    lessons: [{ id: 'lesson', title: 'Read closely', objective: 'Connect a claim to its passage.', estimatedMinutes: 30, preparation: 'outline', blocks: [], completed: false }],
  }],
} as unknown as LearningProgramDto;

const evidence = {
  reviewStatus: 'passed', contentSha256: 'hash', unavailableCount: 0,
  citations: [{
    path: '/modules/0/summary', target: 'module_summary', moduleId: 'module', lessonId: null, itemIndex: null,
    claim: 'A sourced summary.', sourceId: 'source', sourceTitle: 'Saved field guide', sourceUrl: 'https://example.com/guide',
    quote: 'This is the exact frozen passage supporting the visible summary.',
  }],
} as LearningOutlineEvidenceDto;

describe('course syllabus citations', () => {
  it('opens the exact saved passage from the claim it supports', async () => {
    render(<CourseSyllabus program={program} moduleId="module" evidence={evidence} onSelect={vi.fn()} onSelectModule={vi.fn()} />);

    expect(screen.getByText('A sourced summary.')).toBeVisible();
    await userEvent.click(screen.getByText('Saved field guide'));
    expect(screen.getByText('This is the exact frozen passage supporting the visible summary.')).toBeVisible();
    expect(screen.getByRole('link', { name: /Open original/ })).toHaveAttribute('href', 'https://example.com/guide');
  });

  it('withholds cached evidence when the visible claim has changed', () => {
    const revised = { ...program, modules: [{ ...program.modules[0], summary: 'A revised claim without saved evidence.' }] };
    render(<CourseSyllabus program={revised} moduleId="module" evidence={evidence} onSelect={vi.fn()} onSelectModule={vi.fn()} />);

    expect(screen.getByText('A revised claim without saved evidence.')).toBeVisible();
    expect(screen.queryByText('Saved field guide')).not.toBeInTheDocument();
  });
});

const lesson = (id: string, title: string, completed = false) => ({ id, title, objective: `Objective of ${title}.`, estimatedMinutes: 20, preparation: 'outline', blocks: [], completed });
const journey = {
  summary: { id: 'program', title: 'Evidence course', status: 'draft' },
  modules: [
    { id: 'm1', title: 'Foundations', summary: 'Start here.', outcomes: [], prerequisiteModuleIds: [], project: null, lessons: [lesson('l1', 'Read closely', true), lesson('l2', 'Weigh sources')] },
    { id: 'm2', title: 'Method', summary: 'Then this.', outcomes: [], prerequisiteModuleIds: [], project: null, lessons: [lesson('l3', 'Design a check')] },
    { id: 'm3', title: 'Capstone', summary: 'Last.', outcomes: [], prerequisiteModuleIds: [], project: null, lessons: [] },
  ],
} as unknown as LearningProgramDto;

function renderJourney(props: Partial<ComponentProps<typeof CourseSyllabus>> = {}) {
  const handlers = { onSelect: vi.fn(), onSelectModule: vi.fn() };
  render(<CourseSyllabus program={journey} moduleId="m1" {...handlers} {...props} />);
  return handlers;
}

describe('course journey navigation', () => {
  it('opens a module from its title', async () => {
    const { onSelect, onSelectModule } = renderJourney();
    await userEvent.click(screen.getByRole('button', { name: 'Method' }));
    expect(onSelectModule).toHaveBeenCalledWith(journey.modules[1]);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('opens a module that has no lessons yet', async () => {
    const { onSelectModule } = renderJourney();
    await userEvent.click(screen.getByRole('button', { name: 'Capstone' }));
    expect(onSelectModule).toHaveBeenCalledWith(journey.modules[2]);
  });

  it('lets the keyboard reach and open a module', async () => {
    const user = userEvent.setup();
    const { onSelectModule } = renderJourney();
    const method = screen.getByRole('button', { name: 'Method' });
    method.focus();
    await user.keyboard('{Enter}');
    expect(onSelectModule).toHaveBeenCalledTimes(1);
    await user.keyboard(' ');
    expect(onSelectModule).toHaveBeenCalledTimes(2);
    expect(onSelectModule).toHaveBeenLastCalledWith(journey.modules[1]);
  });

  it('opens a lesson in its own module, and not the module on its own', async () => {
    const { onSelect, onSelectModule } = renderJourney();
    await userEvent.click(screen.getByRole('button', { name: /Weigh sources/ }));
    expect(onSelect).toHaveBeenCalledWith(journey.modules[0], 'l2');
    expect(onSelectModule).not.toHaveBeenCalled();
  });

  it('marks the module you are in', () => {
    renderJourney({ moduleId: 'm2' });
    expect(screen.getByRole('button', { name: 'Method' })).toHaveAttribute('aria-current', 'true');
    expect(screen.getByRole('button', { name: 'Foundations' })).not.toHaveAttribute('aria-current');
  });

  it('closes the dropdown once a module is chosen from it', async () => {
    const user = userEvent.setup();
    const active = { ...journey, summary: { ...journey.summary, status: 'active' } } as LearningProgramDto;
    const { onSelectModule } = renderJourney({ compact: true, program: active });
    const trigger = screen.getByRole('button', { name: /Your course journey/ });
    expect(screen.queryByRole('button', { name: 'Method' })).not.toBeInTheDocument();
    await user.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await user.click(screen.getByRole('button', { name: 'Method' }));
    expect(onSelectModule).toHaveBeenCalledWith(active.modules[1]);
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
  });
});
