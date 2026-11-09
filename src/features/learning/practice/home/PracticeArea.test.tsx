import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { PracticeArea } from './PracticeArea';
import { usePracticeView } from './usePracticeView';

import type { HistoryTarget } from './historyItems';

vi.mock('./PracticeHome', () => ({
  PracticeHome: ({ onOpenTarget, onOpenWorkbench }: { onOpenTarget: (target: HistoryTarget) => void; onOpenWorkbench: () => void }) => (
    <div>
      <button type="button" onClick={() => onOpenTarget({ kind: 'attempt', sessionId: 'sess-9', lessonId: 'lesson-2', mode: 'practice' })}>open attempt</button>
      <button type="button" onClick={() => onOpenTarget({ kind: 'drill', attemptId: 'att-3' })}>open drill</button>
      <button type="button" onClick={onOpenWorkbench}>open workbench</button>
    </div>
  ),
}));
vi.mock('@/features/learning/practice/PracticeWorkbenchPanel', () => ({
  PracticeWorkbenchPanel: ({ lesson, attemptId }: { lesson?: { id: string }; attemptId?: string | null }) => <p>{`Written ${lesson?.id} ${attemptId ?? 'new'}`}</p>,
}));
vi.mock('@/features/learning/practice/FeynmanTechniquePanel', () => ({ FeynmanTechniquePanel: () => <p>Feynman</p> }));
vi.mock('@/features/learning/practice/practical/PracticalWorkbenchPanel', () => ({
  PracticalWorkbenchPanel: ({ activityId }: { activityId?: string | null }) => <p>{`Practical ${activityId ?? 'none'}`}</p>,
}));
vi.mock('@/features/learning/drills/CourseDrillsPanel', () => ({
  CourseDrillsPanel: ({ attemptId }: { attemptId?: string | null }) => <p>{`Drills ${attemptId ?? 'new'}`}</p>,
}));
vi.mock('@/features/learning/practice/coding/CodingEntry', () => ({
  CodingEntry: ({ choice }: { choice: { language: string } }) => <p>{`Coding build ${choice.language}`}</p>,
}));
vi.mock('@/features/learning/practice/coding/CodingSession', () => ({
  CodingSession: ({ onTryInterview }: { onTryInterview: (setting: 'mock') => void }) => (
    <button type="button" onClick={() => onTryInterview('mock')}>try interview</button>
  ),
}));
vi.mock('./practiceHomeApi', () => ({ practiceHomeApi: { startCodingSession: vi.fn() } }));

const lesson = (id: string) => ({ id, title: id }) as Wire.LearningLessonDto;
const program = {
  summary: { id: 'course-1', goal: 'Learn' },
  modules: [{ id: 'm', lessons: [lesson('lesson-1'), lesson('lesson-2')] }],
} as unknown as Wire.LearningProgramDto;

function Harness() {
  const view = usePracticeView();
  return <PracticeArea program={program} currentLesson={lesson('lesson-1')} currentModule={undefined} view={view} />;
}

const tabs = () => screen.getAllByRole('tab');

describe('PracticeArea', () => {
  it('reopens a written attempt on its own session and its own lesson', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'open attempt' }));
    expect(screen.getByText('Written lesson-2 sess-9')).toBeVisible();
  });

  it('reopens a drill on its own attempt', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'open drill' }));
    expect(screen.getByText('Drills att-3')).toBeVisible();
  });

  it('tries an interview in Python even when another language was remembered for the Code card', async () => {
    localStorage.setItem('lattice:practice:sentence:course-1:code', JSON.stringify({ language: 'javascript' }));
    const user = userEvent.setup();
    function Open() {
      const view = usePracticeView();
      return (
        <>
          <button type="button" onClick={() => view.openCoding('s1')}>open session</button>
          <PracticeArea program={program} currentLesson={lesson('lesson-1')} currentModule={undefined} view={view} />
        </>
      );
    }
    render(<Open />);
    await user.click(screen.getByRole('button', { name: 'open session' }));
    await user.click(screen.getByRole('button', { name: 'try interview' }));
    expect(screen.getByText('Coding build python')).toBeVisible();
    localStorage.clear();
  });

  it('opens the practical workbench with no activity selected from the workbench button', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'open workbench' }));
    expect(screen.getByText('Practical none')).toBeVisible();
  });

  describe('Practice activities tabs', () => {
    async function open() {
      const user = userEvent.setup();
      render(<Harness />);
      await user.click(screen.getByRole('button', { name: 'open workbench' }));
      return user;
    }

    it('keeps only the selected tab in the tab order', async () => {
      await open();
      expect(tabs().map((tab) => tab.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false']);
      expect(tabs().map((tab) => tab.tabIndex)).toEqual([-1, 0, -1]);
    });

    it('moves and selects with the arrow keys, wrapping at both ends', async () => {
      const user = await open();
      tabs()[1]!.focus();
      await user.keyboard('{ArrowRight}');
      expect(tabs()[2]).toHaveFocus();
      expect(tabs()[2]).toHaveAttribute('aria-selected', 'true');
      await user.keyboard('{ArrowRight}');
      expect(tabs()[0]).toHaveFocus();
      expect(tabs()[0]).toHaveAttribute('aria-selected', 'true');
      await user.keyboard('{ArrowLeft}');
      expect(tabs()[2]).toHaveFocus();
    });

    it('goes to the first and last tab with Home and End', async () => {
      const user = await open();
      tabs()[1]!.focus();
      await user.keyboard('{End}');
      expect(tabs()[2]).toHaveFocus();
      await user.keyboard('{Home}');
      expect(tabs()[0]).toHaveFocus();
      expect(tabs()[0]).toHaveAttribute('aria-selected', 'true');
    });

    it('ties each tab to a tabpanel that is named by it', async () => {
      const user = await open();
      await user.click(tabs()[0]!);
      await user.click(tabs()[2]!);
      for (const tab of tabs()) {
        const panel = document.getElementById(tab.getAttribute('aria-controls') ?? '');
        expect(panel).not.toBeNull();
        expect(panel).toHaveAttribute('role', 'tabpanel');
        expect(panel).toHaveAttribute('aria-labelledby', tab.id);
      }
    });

    it('shows only the selected tab\'s panel and keeps the others\' work mounted', async () => {
      const user = await open();
      await user.click(tabs()[2]!);
      expect(screen.getByText('Drills new')).toBeVisible();
      expect(screen.getByText('Practical none')).not.toBeVisible();
    });
  });
});
