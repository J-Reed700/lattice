import type { ReactNode } from 'react';

import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { LearningTangentDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import {
  MAX_STUDY_TANGENT_PASSAGE_LENGTH,
  SectionTangentBar,
  StudyTangentSelection,
  StudyTangentsContext,
  useStudyTangentsContext,
  type StudyTangentsContextValue,
} from './StudyTangentSelection';

const PASSAGE = 'The Nile flooded every summer, leaving rich silt along its banks.';

function contextValue(overrides: Partial<StudyTangentsContextValue> = {}): StudyTangentsContextValue {
  return {
    programId: 'course-1', tangents: [], activeTangentId: null, isCreating: false,
    startTangent: vi.fn().mockResolvedValue(undefined), openTangent: vi.fn(), openTangentsDrawer: vi.fn(), ...overrides,
  };
}

function renderSelection(context: StudyTangentsContextValue | null = contextValue(), text = PASSAGE) {
  const view = render(
    <StudyTangentsContext.Provider value={context}>
      <article tabIndex={-1} aria-label="Lesson">
        <StudyTangentSelection moduleId="m1" lessonId="l1" sectionIndex={2} sectionTitle="Floods">
          <p data-testid="passage">{text}</p>
        </StudyTangentSelection>
      </article>
      <p data-testid="outside">Sidebar note</p>
    </StudyTangentsContext.Provider>,
  );
  return { ...view, context };
}

function select(node: Node, start = 0, end = node.textContent?.length ?? 0) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  act(() => { document.dispatchEvent(new Event('selectionchange')); });
}

const passageText = () => screen.getByTestId('passage').firstChild!;
const toolbar = () => screen.queryByRole('toolbar', { name: 'Selected passage' });

beforeEach(() => {
  vi.spyOn(Range.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(40, 100, 200, 20));
  vi.spyOn(toast, 'error');
});
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  vi.restoreAllMocks();
});

describe('StudyTangentSelection', () => {
  it('requires the tangents context for its hook', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => renderHook(() => useStudyTangentsContext())).toThrow('useStudyTangentsContext must be used within a StudyTangentsContext.Provider');
    const wrapper = ({ children }: { children: ReactNode }) => <StudyTangentsContext.Provider value={contextValue()}>{children}</StudyTangentsContext.Provider>;
    expect(renderHook(() => useStudyTangentsContext(), { wrapper }).result.current.programId).toBe('course-1');
  });

  it('renders just the lesson without tangents', () => {
    renderSelection(null);
    select(passageText(), 4, 8);
    expect(screen.getByTestId('passage')).toBeInTheDocument();
    expect(toolbar()).not.toBeInTheDocument();
  });

  it('offers a tangent on a passage selected in the lesson, below it', () => {
    const { context } = renderSelection();
    select(passageText(), 4, 8);
    expect(toolbar()).toHaveStyle({ left: '40px', top: '128px' });
    fireEvent.click(screen.getByRole('button', { name: 'Ask in a tangent' }));
    expect(context!.startTangent).toHaveBeenCalledWith({ moduleId: 'm1', lessonId: 'l1', sectionIndex: 2, sectionTitle: 'Floods', selectedText: 'Nile' });
    expect(toolbar()).not.toBeInTheDocument();
    expect(window.getSelection()?.rangeCount).toBe(0);
  });

  it('ignores selections outside the lesson or of whitespace', () => {
    renderSelection();
    select(screen.getByTestId('outside').firstChild!, 0, 7);
    expect(toolbar()).not.toBeInTheDocument();
    select(passageText(), 8, 9);
    expect(toolbar()).not.toBeInTheDocument();
  });

  it('waits for a pointer selection to finish before offering it', () => {
    renderSelection();
    fireEvent.pointerDown(document, { button: 2 });
    fireEvent.pointerDown(document, { button: 0 });
    select(passageText(), 4, 8);
    expect(toolbar()).not.toBeInTheDocument();
    fireEvent.pointerUp(document);
    expect(toolbar()).toBeInTheDocument();
    fireEvent.pointerDown(document, { button: 0 });
    expect(toolbar()).not.toBeInTheDocument();
    fireEvent.pointerCancel(document);
    expect(toolbar()).not.toBeInTheDocument();
  });

  it('goes away on Escape, a click elsewhere or a scroll, but not a click on itself', () => {
    renderSelection();
    select(passageText(), 4, 8);
    fireEvent.keyDown(document, { key: 'Enter' });
    expect(toolbar()).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(toolbar()).not.toBeInTheDocument();

    select(passageText(), 4, 9);
    fireEvent.pointerDown(toolbar()!);
    expect(toolbar()).toBeInTheDocument();
    fireEvent.pointerDown(screen.getByTestId('outside'));
    expect(toolbar()).not.toBeInTheDocument();

    select(passageText(), 4, 10);
    act(() => { document.dispatchEvent(new Event('scroll')); });
    expect(toolbar()).not.toBeInTheDocument();
  });

  it('goes away when the window is resized', () => {
    // Catch the listener's exception here so it fails this test, not the run.
    const thrown: unknown[] = [];
    const onError = (event: ErrorEvent) => { thrown.push(event.error); event.preventDefault(); };
    window.addEventListener('error', onError);
    try {
      renderSelection();
      select(passageText(), 4, 8);
      act(() => { window.dispatchEvent(new Event('resize')); });
      expect(thrown).toEqual([]);
      expect(toolbar()).not.toBeInTheDocument();
    } finally {
      window.removeEventListener('error', onError);
    }
  });

  it('refuses a passage longer than a tangent can start from', () => {
    const { context } = renderSelection(contextValue(), 'a'.repeat(MAX_STUDY_TANGENT_PASSAGE_LENGTH + 1));
    select(passageText());
    expect(screen.getByText('Select a shorter passage (up to 8,000 characters)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Ask in a tangent' })).toBeDisabled();
    expect(context!.startTangent).not.toHaveBeenCalled();
  });

  it('opens a menu on right-click with tangent and copy, moving focus with the arrows', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    renderSelection();
    select(passageText(), 4, 8);
    fireEvent.contextMenu(screen.getByTestId('passage'), { clientX: 300, clientY: 200 });
    const menu = screen.getByRole('menu', { name: 'Selected passage' });
    expect(menu).toHaveStyle({ left: '300px', top: '200px' });
    const [tangent, copy] = screen.getAllByRole('menuitem');
    expect(tangent).toHaveFocus();
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(copy).toHaveFocus();
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(tangent).toHaveFocus();
    fireEvent.keyDown(menu, { key: 'ArrowUp' });
    expect(copy).toHaveFocus();
    // A later selection change does not replace an open menu.
    act(() => { document.dispatchEvent(new Event('selectionchange')); });
    expect(screen.getByRole('menu')).toBeInTheDocument();
    fireEvent.click(copy);
    expect(writeText).toHaveBeenCalledWith('Nile');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('reports a passage that could not be copied', async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } });
    renderSelection();
    select(passageText(), 4, 8);
    fireEvent.contextMenu(screen.getByTestId('passage'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't copy the passage"));
  });

  it('closes the menu with Escape or Tab and returns focus to the lesson', () => {
    renderSelection();
    select(passageText(), 4, 8);
    fireEvent.contextMenu(screen.getByTestId('passage'));
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Tab' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(screen.getByRole('article', { name: 'Lesson' })).toHaveFocus();

    select(passageText(), 4, 8);
    fireEvent.contextMenu(screen.getByTestId('passage'));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('leaves the browser menu alone without a selection', () => {
    renderSelection();
    fireEvent.contextMenu(screen.getByTestId('passage'));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});

describe('SectionTangentBar', () => {
  const tangent = (id: string, sectionIndex: number | null, lessonId = 'l1') => ({ conversationId: id, title: `Tangent ${id}`, lessonId, sectionIndex } as LearningTangentDto);

  function renderBar(context: StudyTangentsContextValue | null, props: Partial<Parameters<typeof SectionTangentBar>[0]> = {}) {
    render(
      <StudyTangentsContext.Provider value={context}>
        <SectionTangentBar moduleId="m1" lessonId="l1" sectionIndex={2} sectionTitle="Floods" sectionText={`  ${PASSAGE}  `} {...props} />
      </StudyTangentsContext.Provider>,
    );
  }

  it('renders nothing without tangents', () => {
    const { container } = render(<SectionTangentBar moduleId="m1" lessonId="l1" sectionText="x" />);
    expect(container).toBeEmptyDOMElement();
  });

  it('lists this section’s tangents and opens them', () => {
    const context = contextValue({ tangents: [tangent('a', 2), tangent('b', 2), tangent('c', 2), tangent('d', 1), tangent('e', 2, 'l9')] });
    renderBar(context);
    fireEvent.click(screen.getByRole('button', { name: /3 tangents/ }));
    expect(context.openTangentsDrawer).toHaveBeenCalledWith('l1');
    expect(screen.queryByRole('button', { name: 'Tangent c' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Tangent b' }));
    expect(context.openTangent).toHaveBeenCalledWith('b');
  });

  it('names a single tangent in the singular, and the lesson-wide bar by default', () => {
    renderBar(contextValue({ tangents: [tangent('a', null)] }), { sectionIndex: null });
    expect(screen.getByRole('button', { name: /1 tangent$/ })).toBeInTheDocument();
    expect(screen.getByTestId('lesson-tangent-btn')).toHaveTextContent('Start a tangent on this lesson');
  });

  it('starts a tangent from the whole section, trimmed', () => {
    const context = contextValue();
    renderBar(context, { label: 'Explore' });
    fireEvent.click(screen.getByTestId('section-tangent-btn-2'));
    expect(context.startTangent).toHaveBeenCalledWith({ moduleId: 'm1', lessonId: 'l1', sectionIndex: 2, sectionTitle: 'Floods', selectedText: PASSAGE });
    expect(screen.getByTestId('section-tangent-btn-2')).toHaveTextContent('Explore');
  });

  it('does nothing for an empty section and is disabled while a tangent is being made', () => {
    const context = contextValue({ isCreating: true });
    renderBar(context, { sectionText: '   ' });
    expect(screen.getByTestId('section-tangent-btn-2')).toBeDisabled();
    const idle = contextValue();
    renderBar(idle, { sectionText: '   ', sectionIndex: 3 });
    fireEvent.click(screen.getByTestId('section-tangent-btn-3'));
    expect(idle.startTangent).not.toHaveBeenCalled();
  });

  it('asks for a shorter passage from a long section', () => {
    const context = contextValue();
    const long = 'x'.repeat(MAX_STUDY_TANGENT_PASSAGE_LENGTH + 10);
    renderBar(context, { sectionText: long });
    fireEvent.click(screen.getByTestId('section-tangent-btn-2'));
    const field = screen.getByRole('textbox', { name: 'Passage to explore' });
    expect(field).toHaveValue('x'.repeat(4000));
    expect(screen.getByText(`${(4000).toLocaleString()} / 8,000 characters`)).toBeInTheDocument();

    fireEvent.change(field, { target: { value: '   ' } });
    expect(screen.getByRole('button', { name: 'Start tangent' })).toBeDisabled();
    fireEvent.change(field, { target: { value: '  The delta  ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Start tangent' }));
    expect(context.startTangent).toHaveBeenCalledWith(expect.objectContaining({ selectedText: 'The delta' }));
    expect(screen.queryByRole('textbox', { name: 'Passage to explore' })).not.toBeInTheDocument();
  });

  it('closes the passage chooser without starting anything', () => {
    const context = contextValue();
    renderBar(context, { sectionText: 'y'.repeat(MAX_STUDY_TANGENT_PASSAGE_LENGTH + 1) });
    fireEvent.click(screen.getByTestId('section-tangent-btn-2'));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(context.startTangent).not.toHaveBeenCalled();
  });
});
