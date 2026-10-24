import { act, fireEvent, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RetentionFrameworksModal, RETENTION_FRAMEWORKS } from '../RetentionFrameworksModal';
import { StudyHabitsBar } from '../StudyHabitsBar';
import { CHECKLIST_ITEMS, HABIT_CHECKLIST_STORAGE_KEY } from '../types';
import { useHabitChecklist } from '../useHabitChecklist';

import type { FrameworkAction } from '../types';

describe('StudyHabitsBar and Pomodoro Focus Timer', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
    window.localStorage.clear();
  });

  it('renders with 30-min preset selected by default and shows correct initial readout', () => {
    render(<StudyHabitsBar />);

    expect(screen.getByText('30:00')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '30 min' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '45 min' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '25 min' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '5 min Break' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start focus timer' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reset timer' })).toBeInTheDocument();
  });

  it('switches between presets (45 min, 25 min, 5 min Break, 30 min)', () => {
    render(<StudyHabitsBar />);

    fireEvent.click(screen.getByRole('button', { name: '45 min' }));
    expect(screen.getByText('45:00')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '25 min' }));
    expect(screen.getByText('25:00')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '5 min Break' }));
    expect(screen.getByText('05:00')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '30 min' }));
    expect(screen.getByText('30:00')).toBeInTheDocument();
  });

  it('starts, ticks down, pauses, and resets the Pomodoro timer', () => {
    vi.useFakeTimers();
    render(<StudyHabitsBar />);

    const startButton = screen.getByRole('button', { name: 'Start focus timer' });
    fireEvent.click(startButton);

    // Readout decrements
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('29:59')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(59000);
    });
    expect(screen.getByText('29:00')).toBeInTheDocument();

    // Pause timer
    const pauseButton = screen.getByRole('button', { name: 'Pause focus timer' });
    fireEvent.click(pauseButton);

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    // Remains paused
    expect(screen.getByText('29:00')).toBeInTheDocument();

    // Reset timer
    const resetButton = screen.getByRole('button', { name: 'Reset timer' });
    fireEvent.click(resetButton);
    expect(screen.getByText('30:00')).toBeInTheDocument();
  });

  it('stops at 00:00 when countdown completes', () => {
    vi.useFakeTimers();
    render(<StudyHabitsBar />);

    fireEvent.click(screen.getByRole('button', { name: '5 min Break' }));
    expect(screen.getByText('05:00')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Start focus timer' }));

    act(() => {
      vi.advanceTimersByTime(300 * 1000);
    });

    expect(screen.getByText('00:00')).toBeInTheDocument();
    // After finishing, clicking start resets back to active preset duration
    const restartButton = screen.getByRole('button', { name: 'Start focus timer' });
    expect(restartButton).toBeInTheDocument();
    fireEvent.click(restartButton);
    expect(screen.getByText('05:00')).toBeInTheDocument();
  });

  it('triggers Anti-Cramming & Single-Tasking Guard alert after 90 continuous study minutes', () => {
    vi.useFakeTimers();
    render(<StudyHabitsBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Start focus timer' }));

    // Advance 91 minutes of active study
    act(() => {
      vi.advanceTimersByTime(91 * 60 * 1000);
    });

    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(
      screen.getByText(/Anti-Cramming & Single-Tasking Guard:/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/You have been studying for 91 continuous minutes without a break/i)
    ).toBeInTheDocument();

    // Clicking "Take 5-min Break" resets continuous study and switches to 5-min break
    fireEvent.click(screen.getByRole('button', { name: /Take 5-min Break/i }));
    expect(screen.getByText('05:00')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('allows dismissing the anti-cramming alert', () => {
    vi.useFakeTimers();
    render(<StudyHabitsBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Start focus timer' }));

    act(() => {
      vi.advanceTimersByTime(91 * 60 * 1000);
    });

    expect(screen.getByRole('alert')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('toggles focus guard single-tasking banner', () => {
    render(<StudyHabitsBar />);

    const toggle = screen.getByTitle('Toggle focus guard: avoid multitasking & cramming');
    expect(
      screen.queryByText(/Single-tasking active: one tab, no phone or notifications/i)
    ).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(
      screen.getByText(/Single-tasking active: one tab, no phone or notifications/i)
    ).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(
      screen.queryByText(/Single-tasking active: one tab, no phone or notifications/i)
    ).not.toBeInTheDocument();
  });
});

describe('RetentionFrameworksModal and Core Retention Strategies', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it('exports RETENTION_FRAMEWORKS metadata containing all 6 core evidence-based strategies', () => {
    expect(RETENTION_FRAMEWORKS).toHaveLength(6);
    const ids = RETENTION_FRAMEWORKS.map((f) => f.id);
    expect(ids).toEqual([
      'recall-dump',
      'spaced-repetition',
      'feynman',
      'interleaving',
      'dual-coding',
      'handwritten-notes',
    ]);
  });

  it('opens RetentionFrameworksModal from StudyHabitsBar and renders all 6 core strategies', async () => {
    const user = userEvent.setup();
    const handleSelect = vi.fn();
    render(<StudyHabitsBar onSelectFramework={handleSelect} />);

    const openGuideButton = screen.getByRole('button', {
      name: /Learning Frameworks & Retention Guide/i,
    });
    await user.click(openGuideButton);

    expect(screen.getByText('Evidence-Based Learning & Retention Guide')).toBeInTheDocument();

    // Verify 1: Active Recall
    expect(screen.getByText('Active Recall (Testing Yourself)')).toBeInTheDocument();
    expect(
      screen.getByText(/Close notes and write\/speak everything you remember from scratch/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Free Recall (Brain Dump)' })
    ).toBeInTheDocument();

    // Verify 2: Spaced Repetition
    expect(screen.getByText('Spaced Repetition')).toBeInTheDocument();
    expect(
      screen.getByText(/Combat the 48-hour forgetting curve with expanding intervals/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Spaced Repetition & Curve' })
    ).toBeInTheDocument();

    // Verify 3: The Feynman Technique
    expect(screen.getByText('The Feynman Technique / Teaching Others')).toBeInTheDocument();
    expect(
      screen.getByText(/Explain concepts in simple, everyday language as if teaching a beginner/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Feynman Workbench' })
    ).toBeInTheDocument();

    // Verify 4: Interleaving
    expect(screen.getByText('Interleaving')).toBeInTheDocument();
    expect(
      screen.getByText(/Mix different sub-topics, decks, or problem types in a single session/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Interleaved Practice' })
    ).toBeInTheDocument();

    // Verify 5: Dual Coding & Visualization
    expect(screen.getByText('Dual Coding & Visualization')).toBeInTheDocument();
    expect(
      screen.getByText(/Pair dense text or abstract numbers with diagrams, Mermaid charts/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Dual-Coding Canvas' })
    ).toBeInTheDocument();

    // Verify 6: Handwritten Notes & Own-Words Synthesis
    expect(screen.getByText('Handwritten Notes & Own-Words Synthesis')).toBeInTheDocument();
    expect(
      screen.getByText(/Write\/sketch key takeaways by hand and rephrase ideas rather than transcribing verbatim/i)
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open Handwritten & Synthesis Notebook' })
    ).toBeInTheDocument();
  });

  it('triggers onSelectFramework callback and onNavigate when an action button is clicked', async () => {
    const user = userEvent.setup();
    const handleSelect = vi.fn();
    const handleNavigate = vi.fn();
    const handleOpenChange = vi.fn();

    render(
      <RetentionFrameworksModal
        open
        onOpenChange={handleOpenChange}
        onSelectFramework={handleSelect}
        onNavigate={handleNavigate}
      />
    );

    // Click Active Recall action button
    const recallButton = screen.getByRole('button', { name: 'Open Free Recall (Brain Dump)' });
    await user.click(recallButton);

    expect(handleSelect).toHaveBeenCalledWith('recall-dump');
    expect(handleNavigate).toHaveBeenCalledWith('recall-dump');
    expect(handleOpenChange).toHaveBeenCalledWith(false);
  });

  it('triggers onSelectFramework for each of the other framework action buttons', async () => {
    const user = userEvent.setup();
    const actions: { name: string; expected: FrameworkAction }[] = [
      { name: 'Open Spaced Repetition & Curve', expected: 'spaced-repetition' },
      { name: 'Open Feynman Workbench', expected: 'feynman' },
      { name: 'Open Interleaved Practice', expected: 'interleaving' },
      { name: 'Open Dual-Coding Canvas', expected: 'dual-coding' },
      { name: 'Open Handwritten & Synthesis Notebook', expected: 'handwritten-notes' },
    ];

    for (const item of actions) {
      const handleSelect = vi.fn();
      const { unmount } = render(
        <RetentionFrameworksModal
          open
          onOpenChange={vi.fn()}
          onSelectFramework={handleSelect}
        />
      );

      const btn = screen.getByRole('button', { name: item.name });
      await user.click(btn);
      expect(handleSelect).toHaveBeenCalledWith(item.expected);
      unmount();
    }
  });
});

describe('Harvard Retention Principles, Mayo Clinic Health & Metacognitive Checklist', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it('renders Harvard Summer School and Mayo Clinic resources with proper external links', () => {
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);

    expect(
      screen.getByText('Harvard Summer School Retention Principles')
    ).toBeInTheDocument();

    const harvardLink = screen.getByRole('link', { name: /Read Harvard Guide/i });
    expect(harvardLink).toHaveAttribute(
      'href',
      'https://summer.harvard.edu/blog/5-tips-to-retain-what-you-learn/'
    );
    expect(harvardLink).toHaveAttribute('target', '_blank');
    expect(harvardLink).toHaveAttribute('rel', 'noreferrer');

    expect(
      screen.getByText('Mayo Clinic Physical & Mental Health Memory Foundations')
    ).toBeInTheDocument();

    const mayoLink = screen.getByRole('link', { name: /Read Mayo Clinic Guide/i });
    expect(mayoLink).toHaveAttribute(
      'href',
      'https://www.mayoclinic.org/healthy-lifestyle/healthy-aging/in-depth/memory-loss/art-20046518'
    );
    expect(mayoLink).toHaveAttribute('target', '_blank');
    expect(mayoLink).toHaveAttribute('rel', 'noreferrer');

    // Supportive Habits (Pomodoro & single-tasking)
    expect(
      screen.getByText(/Pomodoro 30–45 Minute Focused Intervals & Single-Tasking Guard/i)
    ).toBeInTheDocument();
  });

  it('renders all 6 required checklist items with exact required descriptions', () => {
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);

    expect(
      screen.getByText('Focused block: Studying in a 30–45 minute undistracted block (no multitasking or cramming)')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Pre-study recall: Tested what I already know before rereading')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Own-words synthesis: Rephrased key takeaways without copying word-for-word')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Sleep & consolidation: Slept 7–8+ hours for synaptic memory consolidation')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Aerobic movement: Completed physical exercise to boost neuroplasticity and brain blood flow')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Social / teaching engagement: Discussed or taught a concept to someone else')
    ).toBeInTheDocument();
  });

  it('toggles checklist items, persists to localStorage under the correct key, and updates progress', async () => {
    const user = userEvent.setup();
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);

    expect(screen.getByText(/0 of 6 completed \(0%\)/i)).toBeInTheDocument();

    const sleepLabel = screen.getByText(
      'Sleep & consolidation: Slept 7–8+ hours for synaptic memory consolidation'
    );
    await user.click(sleepLabel);

    expect(screen.getByText(/1 of 6 completed \(17%\)/i)).toBeInTheDocument();

    const raw = window.localStorage.getItem(HABIT_CHECKLIST_STORAGE_KEY);
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw!);
    expect(parsed['sleep-consolidation']).toBe(true);

    // Toggle another item
    const movementLabel = screen.getByText(
      'Aerobic movement: Completed physical exercise to boost neuroplasticity and brain blood flow'
    );
    await user.click(movementLabel);
    expect(screen.getByText(/2 of 6 completed \(33%\)/i)).toBeInTheDocument();

    // Reset all checklist items
    const resetBtn = screen.getByRole('button', { name: /Reset/i });
    await user.click(resetBtn);
    expect(screen.getByText(/0 of 6 completed \(0%\)/i)).toBeInTheDocument();
    expect(window.localStorage.getItem(HABIT_CHECKLIST_STORAGE_KEY)).toBeNull();
  });

  it('opens habits tab directly when Health & Consolidation pill is clicked in StudyHabitsBar', async () => {
    const user = userEvent.setup();
    render(<StudyHabitsBar />);

    const healthPill = screen.getByTitle('Open Health & Harvard Principles Self-Assessment');
    await user.click(healthPill);

    expect(
      screen.getByText('Metacognitive Self-Assessment & Daily Habits Checklist')
    ).toBeInTheDocument();
    expect(
      screen.getByText('Harvard Summer School Retention Principles')
    ).toBeInTheDocument();
  });

  it('renders completed habit indicators on Health & Consolidation pill when items are checked', () => {
    window.localStorage.setItem(
      HABIT_CHECKLIST_STORAGE_KEY,
      JSON.stringify({
        'sleep-consolidation': true,
        'aerobic-movement': true,
        'focused-block': true,
      })
    );

    render(<StudyHabitsBar />);

    expect(screen.getByLabelText('Sleep completed')).toBeInTheDocument();
    expect(screen.getByLabelText('Aerobic movement completed')).toBeInTheDocument();
    expect(screen.getByLabelText('Focused block completed')).toBeInTheDocument();
  });

  it('allows switching between tabs in RetentionFrameworksModal', async () => {
    const user = userEvent.setup();
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="frameworks" />);

    expect(screen.getByText('Active Recall (Testing Yourself)')).toBeInTheDocument();

    const habitsTabTrigger = screen.getByRole('tab', { name: /Harvard Principles & Health/i });
    await user.click(habitsTabTrigger);

    expect(
      screen.getByText('Metacognitive Self-Assessment & Daily Habits Checklist')
    ).toBeInTheDocument();

    const frameworksTabTrigger = screen.getByRole('tab', { name: /Core Retention Frameworks/i });
    await user.click(frameworksTabTrigger);

    expect(screen.getByText('Active Recall (Testing Yourself)')).toBeInTheDocument();
  });

  it('forwards onNavigate when framework action is chosen via StudyHabitsBar modal', async () => {
    const user = userEvent.setup();
    const handleNavigate = vi.fn();
    render(<StudyHabitsBar onNavigate={handleNavigate} />);

    await user.click(screen.getByRole('button', { name: /Learning Frameworks & Retention Guide/i }));
    await user.click(screen.getByRole('button', { name: 'Open Free Recall (Brain Dump)' }));

    expect(handleNavigate).toHaveBeenCalledWith('recall-dump');
  });
});

describe('useHabitChecklist hook functionality', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  it('manages item state, checks completion, and responds to storage / custom events', () => {
    const { result } = renderHook(() => useHabitChecklist());

    expect(result.current.completedCount).toBe(0);
    expect(result.current.isComplete).toBe(false);

    // setItemState
    act(() => {
      result.current.setItemState('sleep-consolidation', true);
    });
    expect(result.current.completedCount).toBe(1);
    expect(result.current.checklist['sleep-consolidation']).toBe(true);

    // toggleItem
    act(() => {
      result.current.toggleItem('sleep-consolidation');
    });
    expect(result.current.checklist['sleep-consolidation']).toBe(false);

    // Complete all items to verify isComplete
    act(() => {
      for (const item of CHECKLIST_ITEMS) {
        result.current.setItemState(item.id, true);
      }
    });
    expect(result.current.isComplete).toBe(true);
    expect(result.current.completedCount).toBe(result.current.totalCount);

    // resetAll
    act(() => {
      result.current.resetAll();
    });
    expect(result.current.completedCount).toBe(0);
    expect(result.current.isComplete).toBe(false);

    // Simulate external update via storage event
    window.localStorage.setItem(
      HABIT_CHECKLIST_STORAGE_KEY,
      JSON.stringify({ 'aerobic-movement': true })
    );
    act(() => {
      window.dispatchEvent(new Event('storage'));
    });
    expect(result.current.checklist['aerobic-movement']).toBe(true);

    // Simulate lattice custom event
    window.localStorage.setItem(
      HABIT_CHECKLIST_STORAGE_KEY,
      JSON.stringify({ 'focused-block': true })
    );
    act(() => {
      window.dispatchEvent(new CustomEvent('lattice:habit-checklist-update'));
    });
    expect(result.current.checklist['focused-block']).toBe(true);
  });

  it('handles invalid JSON in localStorage safely', () => {
    window.localStorage.setItem(HABIT_CHECKLIST_STORAGE_KEY, 'not-valid-json{{{');
    const { result } = renderHook(() => useHabitChecklist());
    expect(result.current.checklist).toEqual({});
  });
});
