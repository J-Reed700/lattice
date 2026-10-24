import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  computeVerbatimOverlap,
  HandwrittenSynthesisTools,
} from '@/features/learning/memory/HandwrittenSynthesisTools';

describe('computeVerbatimOverlap', () => {
  const sourceTexts = [
    'The core mechanism of photosynthesis involves light dependent reactions where chlorophyll absorbs photons.',
    'Carbon dioxide is fixed into carbohydrates via the Calvin cycle inside the stroma of chloroplasts.',
  ];

  it('returns 0% when note is empty', () => {
    const result = computeVerbatimOverlap('', sourceTexts);
    expect(result.percent).toBe(0);
    expect(result.noteWordCount).toBe(0);
    expect(result.matchedPhrases).toHaveLength(0);
  });

  it('returns 0% when note uses completely original phrasing', () => {
    const note =
      'Plants gather energy from sunshine through green pigments and store that power by assembling sugar molecules from air.';
    const result = computeVerbatimOverlap(note, sourceTexts);
    expect(result.percent).toBe(0);
    expect(result.matchedPhrases).toHaveLength(0);
    expect(result.noteWordCount).toBeGreaterThan(15);
  });

  it('detects high verbatim overlap (> 25%) when 5-word phrases are copied from source', () => {
    const copiedNote =
      'In today study we see the core mechanism of photosynthesis involves light dependent reactions where chlorophyll absorbs photons for fuel.';
    const result = computeVerbatimOverlap(copiedNote, sourceTexts);
    expect(result.percent).toBeGreaterThan(25);
    expect(result.matchedPhrases.length).toBeGreaterThan(0);
    expect(result.matchedPhrases.some((p) => p.includes('core mechanism of photosynthesis involves'))).toBe(true);
  });

  it('handles short notes under 5 words by checking phrase match', () => {
    const shortMatch = computeVerbatimOverlap('Calvin cycle inside', sourceTexts);
    expect(shortMatch.percent).toBe(100);

    const shortNoMatch = computeVerbatimOverlap('Random words outside', sourceTexts);
    expect(shortNoMatch.percent).toBe(0);
  });
});

describe('HandwrittenSynthesisTools UI', () => {
  const sources = [
    'Observation differs fundamentally from inference because an observation records empirical sensory evidence directly.',
  ];

  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();

    HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
      clearRect: vi.fn(),
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
    });
    HTMLCanvasElement.prototype.getBoundingClientRect = vi.fn().mockReturnValue({
      left: 0,
      top: 0,
      width: 720,
      height: 260,
    });
  });

  it('shows minimum word count helper when note has less than 15 words', () => {
    render(
      <HandwrittenSynthesisTools
        noteContent="Short note here."
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    expect(
      screen.getByText(/Write at least 15 words in your own phrasing to evaluate synthesis/i)
    ).toBeInTheDocument();
  });

  it('warns when high verbatim overlap (> 25%) is detected', () => {
    const verbatimNote =
      'Today I learned that observation differs fundamentally from inference because an observation records empirical sensory evidence directly without any extrapolation.';

    render(
      <HandwrittenSynthesisTools
        noteContent={verbatimNote}
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    expect(screen.getByText(/High verbatim overlap/i)).toBeInTheDocument();
    expect(screen.getByText(/High word-for-word overlap detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Copied source phrases to rephrase:/i)).toBeInTheDocument();
  });

  it('shows success badge when note has >= 15 words and original phrasing', () => {
    const originalNote =
      'Whenever analyzing data, always distinguish raw seen facts from the explanations or stories constructed to interpret those facts.';

    render(
      <HandwrittenSynthesisTools
        noteContent={originalNote}
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    expect(screen.getByText(/Synthesized in your own words/i)).toBeInTheDocument();
  });

  it('opens sketchpad, changes tools, draws on canvas, and clears pad', async () => {
    const user = userEvent.setup();
    render(
      <HandwrittenSynthesisTools
        noteContent="Synthesized note about drawing and synthesis."
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    const toggleButton = screen.getByRole('button', { name: /Open sketchpad/i });
    expect(toggleButton).toHaveAttribute('aria-expanded', 'false');
    await user.click(toggleButton);

    expect(toggleButton).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(/Write key takeaways down by hand instead of typing/i)).toBeInTheDocument();

    // Select blue color
    const blueButton = screen.getByTitle('Blue');
    await user.click(blueButton);

    // Toggle highlighter
    const highlighterButton = screen.getByRole('button', { name: /Highlighter/i });
    await user.click(highlighterButton);

    // Canvas drawing interaction via pointer events
    const canvas = screen.getByLabelText('Handwritten sketchpad canvas');
    expect(canvas).toBeInTheDocument();

    fireEvent.pointerDown(canvas, { clientX: 50, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(canvas, { clientX: 70, clientY: 80, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: 70, clientY: 80, pointerId: 1 });

    // Check undo button
    const undoButton = screen.getByRole('button', { name: /Undo/i });
    expect(undoButton).not.toBeDisabled();
    await user.click(undoButton);

    // Check clear button
    const clearButton = screen.getByRole('button', { name: /Clear/i });
    await user.click(clearButton);
    expect(clearButton).toBeDisabled();
  });

  it('attaches handwritten takeaway to note via onInsertSynthesis callback', async () => {
    const onInsertMock = vi.fn();
    const user = userEvent.setup();

    render(
      <HandwrittenSynthesisTools
        noteContent="Existing note content."
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
        onInsertSynthesis={onInsertMock}
      />
    );

    await user.click(screen.getByRole('button', { name: /Open sketchpad/i }));

    const takeawayInput = screen.getByPlaceholderText(/Transcribe key takeaway from handwriting/i);
    await user.type(takeawayInput, 'Never confuse raw measurements with interpretations.');

    const attachButton = screen.getByRole('button', { name: /Attach Takeaway to Note/i });
    await user.click(attachButton);

    expect(onInsertMock).toHaveBeenCalledTimes(1);
    const insertedArg = onInsertMock.mock.calls[0][0];
    expect(insertedArg).toContain('[Handwritten Takeaway —');
    expect(insertedArg).toContain('Never confuse raw measurements with interpretations.');
    expect(screen.getByText('Attached!')).toBeInTheDocument();
  });

  it('supports controlled mode via sketchpadOpen and onSketchpadOpenChange', async () => {
    const onSketchpadOpenChange = vi.fn();
    const user = userEvent.setup();

    const { rerender } = render(
      <HandwrittenSynthesisTools
        noteContent="Note content"
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
        sketchpadOpen={false}
        onSketchpadOpenChange={onSketchpadOpenChange}
      />
    );

    const toggleButton = screen.getByRole('button', { name: /Open sketchpad/i });
    expect(toggleButton).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByLabelText('Handwritten sketchpad canvas')).not.toBeInTheDocument();

    await user.click(toggleButton);
    expect(onSketchpadOpenChange).toHaveBeenCalledWith(true);

    rerender(
      <HandwrittenSynthesisTools
        noteContent="Note content"
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
        sketchpadOpen
        onSketchpadOpenChange={onSketchpadOpenChange}
      />
    );

    const hideButton = screen.getByRole('button', { name: /Hide sketchpad/i });
    expect(hideButton).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByLabelText('Handwritten sketchpad canvas')).toBeInTheDocument();

    await user.click(hideButton);
    expect(onSketchpadOpenChange).toHaveBeenCalledWith(false);
  });

  it('toggle button is accessible via section title query and state query', () => {
    render(
      <HandwrittenSynthesisTools
        noteContent="Note content"
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    expect(screen.getByRole('button', { name: /Open sketchpad/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Handwritten Note & Sketch Pad/i })).toBeInTheDocument();
  });
});
