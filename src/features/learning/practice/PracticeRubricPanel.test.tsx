import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { RubricPanel } from './PracticeRubricPanel';

const list = vi.hoisted(() => ({ props: null as null | Record<string, any> }));
vi.mock('@/features/learning/practice/shared/CriteriaList', () => ({
  CriteriaList: (props: Record<string, any>) => {
    list.props = props;
    return null;
  },
}));

const rubric = [{ id: 'c1', title: 'Evidence', description: 'Uses the source.', dimension: 'explanation', maxPoints: 4 }] as unknown as Wire.LearningPracticeSessionDto['rubric'];
const graded = (score: number | null) => ({
  gradeStatus: 'uncertain', graderModel: 'Study grader',
  criteria: [{ criterionId: 'c1', dimension: 'explanation', score, maxPoints: 4, observation: 'Saw a quote.', evidenceQuote: 'a quote' }],
}) as unknown as Wire.LearningPracticeSessionDto['result'];

describe('RubricPanel', () => {
  it('lists the criteria before grading with their worth and no points key', () => {
    render(<RubricPanel rubric={rubric} result={null} />);
    const criterion = list.props?.criteria[0];
    expect(criterion).toMatchObject({ id: 'c1', maxPoints: 4, description: 'Uses the source.' });
    expect('points' in criterion).toBe(false);
    expect(criterion.feedback).toBeUndefined();
  });

  it('keeps a null score as no evidence once graded, with the observation', () => {
    render(<RubricPanel rubric={rubric} result={graded(null)} />);
    const criterion = list.props?.criteria[0];
    expect(criterion.points).toBeNull();
    expect(criterion.feedback).toBe('Saw a quote.');
    expect(criterion.description).toBe('Uses the source.');
  });

  it('carries a graded score', () => {
    render(<RubricPanel rubric={rubric} result={graded(3)} />);
    expect(list.props?.criteria[0].points).toBe(3);
  });
});
