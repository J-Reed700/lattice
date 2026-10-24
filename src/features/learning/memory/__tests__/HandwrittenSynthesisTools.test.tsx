import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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

  it('returns 0% when sourceTexts is empty or contains no words', () => {
    const emptyResult = computeVerbatimOverlap('Some note text here', []);
    expect(emptyResult.percent).toBe(0);
    expect(emptyResult.sourceWordCount).toBe(0);

    const punctResult = computeVerbatimOverlap('Some note text here', ['... !!! ???']);
    expect(punctResult.percent).toBe(0);
    expect(punctResult.sourceWordCount).toBe(0);
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

  afterEach(() => {
    vi.useRealTimers();
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

    // Move or release pointer before drawing does not error or draw
    fireEvent.pointerMove(canvas, { clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: 10, clientY: 10, pointerId: 1 });

    // Draw stroke 1 with highlighter
    fireEvent.pointerDown(canvas, { clientX: 50, clientY: 50, pointerId: 1 });
    fireEvent.pointerMove(canvas, { clientX: 70, clientY: 80, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: 70, clientY: 80, pointerId: 1 });

    // Toggle off highlighter and switch widths
    await user.click(highlighterButton);
    await user.click(screen.getByRole('button', { name: 'Broad' }));
    await user.click(screen.getByRole('button', { name: 'Fine' }));

    // Draw stroke 2 (single dot)
    fireEvent.pointerDown(canvas, { clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: 100, clientY: 100, pointerId: 1 });

    // Draw stroke 3 (pen with move)
    fireEvent.pointerDown(canvas, { clientX: 120, clientY: 120, pointerId: 1 });
    fireEvent.pointerMove(canvas, { clientX: 140, clientY: 150, pointerId: 1 });
    fireEvent.pointerUp(canvas, { clientX: 140, clientY: 150, pointerId: 1 });

    // Check undo button (undoes stroke 3)
    const undoButton = screen.getByRole('button', { name: /Undo/i });
    expect(undoButton).not.toBeDisabled();
    await user.click(undoButton);

    // Check clear button (clears remaining 2 strokes and calls handleClear)
    const clearButton = screen.getByRole('button', { name: /Clear/i });
    expect(clearButton).not.toBeDisabled();
    await user.click(clearButton);
    expect(clearButton).toBeDisabled();
  });

  it('attaches handwritten takeaway to note via onInsertSynthesis callback and resets toast timer', () => {
    vi.useFakeTimers();
    const onInsertMock = vi.fn();

    render(
      <HandwrittenSynthesisTools
        noteContent="Existing note content."
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
        onInsertSynthesis={onInsertMock}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Open sketchpad/i }));

    const attachButton = screen.getByRole('button', { name: /Attach Takeaway to Note/i });
    // Click when takeaway is empty -> does nothing
    fireEvent.click(attachButton);
    expect(onInsertMock).not.toHaveBeenCalled();

    const takeawayInput = screen.getByPlaceholderText(/Transcribe key takeaway from handwriting/i);
    fireEvent.change(takeawayInput, {
      target: { value: 'Never confuse raw measurements with interpretations.' },
    });

    fireEvent.click(attachButton);

    expect(onInsertMock).toHaveBeenCalledTimes(1);
    const insertedArg = onInsertMock.mock.calls[0][0];
    expect(insertedArg).toContain('[Handwritten Takeaway —');
    expect(insertedArg).toContain('Never confuse raw measurements with interpretations.');
    expect(screen.getByText('Attached!')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2600);
    });
    expect(screen.queryByText('Attached!')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('loads previously stored strokes from localStorage on mount and redraws them', () => {
    localStorage.setItem(
      'handwritten_sketch_prog1:lesson1',
      JSON.stringify([
        { points: [{ x: 10, y: 10 }], color: '#facc15', width: 18, isHighlighter: true },
        { points: [{ x: 30, y: 30 }, { x: 50, y: 50 }], color: '#2563eb', width: 3, isHighlighter: false },
        { points: [], color: '#000000', width: 1, isHighlighter: false },
      ])
    );

    render(
      <HandwrittenSynthesisTools
        noteContent="Existing note content."
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
        sketchpadOpen
      />
    );

    const canvas = screen.getByLabelText('Handwritten sketchpad canvas');
    expect(canvas).toBeInTheDocument();
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

  it('handles empty strokes or missing onInsertSynthesis callback gracefully', () => {
    localStorage.setItem('handwritten_sketch_prog1:lesson1', '{"not": "array"}');

    render(
      <HandwrittenSynthesisTools
        noteContent="Note content"
        sourceTexts={sources}
        lessonKey="prog1:lesson1"
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /Open sketchpad/i }));

    // Undo when pad is empty
    const undoBtn = screen.getByRole('button', { name: /Undo/i });
    fireEvent.click(undoBtn);

    // Typing takeaway when onInsertSynthesis is omitted
    const takeawayInput = screen.getByPlaceholderText(/Transcribe key takeaway from handwriting/i);
    fireEvent.change(takeawayInput, { target: { value: 'Takeaway with no callback' } });
    const attachBtn = screen.getByRole('button', { name: /Attach Takeaway to Note/i });
    fireEvent.click(attachBtn);
    expect(screen.queryByText('Attached!')).not.toBeInTheDocument();
  });
});
