import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import { describe, expect, it, vi } from 'vitest';

import { OutlineGenerationProgress } from '@/features/learning/curriculum/OutlineGenerationProgress';
import type { LearningOutlineProgressDto, OutlineStage } from '@/lib/bindings';

/**
 * While the model server cannot be reached the backend keeps the work and
 * retries (`patient_model.rs`), reporting "Waiting for the model server
 * (retrying in Ns)" as the stage's `detail`. The learner needs that said as
 * the state of the whole run, not as a muted line under a spinner.
 */

const SENTENCE = 'Can\'t reach the model server. Lattice keeps trying.';
const WAITING = 'Waiting for the model server (retrying in 12s)';

const progress = (stage: OutlineStage, detail: string | null = null): LearningOutlineProgressDto => (
  { stage, detail, elapsedSeconds: 40, stageSeconds: 40, responseCharacters: 0, modelName: 'Test model' }
);

function SettingsProbe() {
  const location = useLocation();
  return <p data-testid="settings-page">{location.pathname}{location.search}</p>;
}

function Tree({ current, onCancel }: { current: LearningOutlineProgressDto; onCancel: () => void }) {
  return <MemoryRouter><Routes>
    <Route path="/" element={<OutlineGenerationProgress progress={current} deepDive={false} onCancel={onCancel} />} />
    <Route path="/settings" element={<SettingsProbe />} />
  </Routes></MemoryRouter>;
}

describe('outline progress while the model server is out of reach', () => {
  it.each<OutlineStage>(['loading_model', 'drafting', 'reviewing', 'repairing'])('says the server cannot be reached, prominently, in the %s stage', (stage) => {
    render(<Tree current={progress(stage, WAITING)} onCancel={vi.fn()} />);
    const sentence = screen.getByText(SENTENCE);
    expect(sentence).toBeVisible();
    // Not the muted detail line the countdown already lives in.
    expect(sentence.closest('[aria-label="Stage progress"]')).toBeNull();
    expect(sentence.closest('[role="alert"], [role="status"]')).not.toBeNull();
  });

  it('offers Open model settings, which opens the tab holding the chat and Learning model', async () => {
    render(<Tree current={progress('loading_model', WAITING)} onCancel={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Open model settings' }));
    expect(await screen.findByTestId('settings-page')).toHaveTextContent('/settings?tab=downloaded-models');
  });

  it('keeps Cancel generation working beside the notice', async () => {
    const cancel = vi.fn();
    render(<Tree current={progress('drafting', WAITING)} onCancel={cancel} />);
    expect(screen.getByText(SENTENCE)).toBeVisible();
    const button = screen.getByRole('button', { name: 'Cancel generation' });
    expect(button).toBeEnabled();
    await userEvent.click(button);
    expect(cancel).toHaveBeenCalledOnce();
  });

  it('drops the notice when the server answers again', () => {
    const view = render(<Tree current={progress('drafting', WAITING)} onCancel={vi.fn()} />);
    expect(screen.getByText(SENTENCE)).toBeVisible();
    view.rerender(<Tree current={progress('drafting', 'Linked 5 outline items; 8 of 23 decided')} onCancel={vi.fn()} />);
    expect(screen.queryByText(SENTENCE)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open model settings' })).not.toBeInTheDocument();
  });

  it('reads as it does today when nothing is waiting', () => {
    render(<Tree current={progress('loading_model', 'Loading Test model')} onCancel={vi.fn()} />);
    expect(screen.getByRole('status')).toHaveTextContent('Getting the model ready');
    expect(screen.getByLabelText('Stage progress')).toHaveTextContent('Loading Test model');
    expect(screen.queryByText(SENTENCE)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open model settings' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel generation' })).toBeEnabled();
  });
});
