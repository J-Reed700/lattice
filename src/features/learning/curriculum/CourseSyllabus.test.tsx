import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { CourseSyllabus } from '@/features/learning/curriculum/CourseSyllabus';
import type { LearningOutlineEvidenceDto, LearningProgramDto } from '@/lib/bindings';

const program = {
  summary: { id: 'program', title: 'Evidence course', status: 'draft' },
  modules: [{
    id: 'module', title: 'Foundations', summary: 'A sourced summary.', outcomes: ['Trace a claim.'], prerequisiteModuleIds: [], project: null,
    lessons: [{ id: 'lesson', title: 'Read closely', objective: 'Connect a claim to its passage.', estimatedMinutes: 30, preparation: 'outline', blocks: [], questions: [], completed: false }],
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
    render(<CourseSyllabus program={program} moduleId="module" evidence={evidence} onSelect={vi.fn()} />);

    expect(screen.getByText('A sourced summary.')).toBeVisible();
    await userEvent.click(screen.getByText('Saved field guide'));
    expect(screen.getByText('This is the exact frozen passage supporting the visible summary.')).toBeVisible();
    expect(screen.getByRole('link', { name: /Open original/ })).toHaveAttribute('href', 'https://example.com/guide');
  });

  it('withholds cached evidence when the visible claim has changed', () => {
    const revised = { ...program, modules: [{ ...program.modules[0], summary: 'A revised claim without saved evidence.' }] };
    render(<CourseSyllabus program={revised} moduleId="module" evidence={evidence} onSelect={vi.fn()} />);

    expect(screen.getByText('A revised claim without saved evidence.')).toBeVisible();
    expect(screen.queryByText('Saved field guide')).not.toBeInTheDocument();
  });
});
