import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { LearningOutlineProgressDto } from '@/lib/bindings';

import { OutlineGenerationProgress } from './OutlineGenerationProgress';

const progress: LearningOutlineProgressDto = { stage: 'drafting', elapsedSeconds: 120, stageSeconds: 95, responseCharacters: 4321, modelName: 'Test model' };

afterEach(() => vi.useRealTimers());

describe('outline progress', () => {
  it('shows actual activity, stage, elapsed time and a working cancel control', () => {
    const cancel = vi.fn();
    const view = render(<OutlineGenerationProgress progress={progress} deepDive onCancel={cancel} />);
    expect(screen.getByRole('status')).toHaveTextContent('Writing your course outline');
    expect(screen.getByLabelText('Elapsed time')).toHaveTextContent('2:00');
    expect(screen.getByText(/4,321 characters received/)).toBeVisible();
    expect(screen.getByText(/This step is taking a while/)).toBeVisible();
    expect(screen.getByText(/24–60 lessons/)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel generation' }));
    expect(cancel).toHaveBeenCalledOnce();
    view.rerender(<OutlineGenerationProgress progress={{ ...progress, stage: 'repairing', responseCharacters: 0 }} deepDive onCancel={cancel} />);
    expect(screen.getByRole('status')).toHaveTextContent('Revising the course outline');
    expect(screen.queryByText(/4,321/)).not.toBeInTheDocument();
    expect(screen.getByText(/Waiting for the model/)).toBeVisible();
  });

  it('keeps elapsed time moving while awaiting the first update', () => {
    vi.useFakeTimers();
    render(<OutlineGenerationProgress deepDive={false} />);
    act(() => vi.advanceTimersByTime(65000));
    expect(screen.getByLabelText('Elapsed time')).toHaveTextContent('1:05');
    expect(screen.getByRole('status')).toHaveTextContent('Starting your course');
  });

  it('prevents cancellation during the final atomic save', () => {
    render(<OutlineGenerationProgress progress={{ ...progress, stage: 'saving' }} deepDive={false} onCancel={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Cancel generation' })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent('Saving your outline');
  });
});
