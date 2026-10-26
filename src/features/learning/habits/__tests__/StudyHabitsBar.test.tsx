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

  it('suggests a break after 90 continuous study minutes', () => {
    vi.useFakeTimers();
    render(<StudyHabitsBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Start focus timer' }));

    // Advance 91 minutes of active study
    act(() => {
      vi.advanceTimersByTime(91 * 60 * 1000);
    });

    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(
      screen.getByText(/Time for a break:/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/You have been studying for 91 minutes without one/i)
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

  it('opens the guide from StudyHabitsBar and states each technique’s evidence without invented figures', async () => {
    const user = userEvent.setup();
    render(<StudyHabitsBar />);
    await user.click(screen.getByRole('button', { name: 'How to learn so it lasts' }));
    expect(screen.getByRole('heading', { name: 'How to learn so it lasts' })).toBeInTheDocument();
    for (const framework of RETENTION_FRAMEWORKS) {
      expect(screen.getByText(framework.title)).toBeInTheDocument();
      expect(screen.getByText(framework.description)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: framework.actionLabel })).toBeInTheDocument();
    }
    expect(screen.getAllByText('Strong evidence')).toHaveLength(2);
    expect(screen.getAllByText('Moderate evidence')).toHaveLength(3);
    expect(screen.getAllByText('Mixed evidence')).toHaveLength(1);
    // No retention percentages, and no institution's name standing in for a study.
    const text = document.body.textContent ?? '';
    expect(text).not.toMatch(/\d+ ?%|Harvard|Mayo/);
  });

  it('cites every technique and habit with a DOI link', () => {
    for (const framework of RETENTION_FRAMEWORKS) expect(framework.citations.length).toBeGreaterThan(0);
    for (const item of CHECKLIST_ITEMS) expect(item.citations.length).toBeGreaterThan(0);
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} />);
    const recall = screen.getAllByRole('link', { name: 'Roediger & Karpicke, 2006' })[0];
    expect(recall).toHaveAttribute('href', 'https://doi.org/10.1111/j.1467-9280.2006.01693.x');
    expect(recall).toHaveAttribute('target', '_blank');
    expect(recall).toHaveAttribute('rel', 'noreferrer');
  });

  it('triggers onSelectFramework and onNavigate for every technique’s action', async () => {
    const user = userEvent.setup();
    for (const framework of RETENTION_FRAMEWORKS) {
      const handleSelect = vi.fn();
      const handleNavigate = vi.fn();
      const handleOpenChange = vi.fn();
      const { unmount } = render(
        <RetentionFrameworksModal open onOpenChange={handleOpenChange} onSelectFramework={handleSelect} onNavigate={handleNavigate} />
      );
      await user.click(screen.getByRole('button', { name: framework.actionLabel }));
      expect(handleSelect).toHaveBeenCalledWith(framework.id satisfies FrameworkAction);
      expect(handleNavigate).toHaveBeenCalledWith(framework.id);
      expect(handleOpenChange).toHaveBeenCalledWith(false);
      unmount();
    }
  });
});

describe('Study habits checklist', () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it('lists what the evidence does not support', () => {
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);
    expect(screen.getByRole('heading', { name: 'What the evidence does not support' })).toBeInTheDocument();
    expect(screen.getByText(/learning style/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Pashler et al., 2008' })).toHaveAttribute('href', 'https://doi.org/10.1111/j.1539-6053.2009.01038.x');
  });

  it('renders every checklist item', () => {
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);
    for (const item of CHECKLIST_ITEMS) expect(screen.getByText(item.label)).toBeInTheDocument();
  });

  it('toggles checklist items, persists to localStorage under the correct key, and updates progress', async () => {
    const user = userEvent.setup();
    render(<RetentionFrameworksModal open onOpenChange={vi.fn()} initialTab="habits" />);

    expect(screen.getByText('0 of 6')).toBeInTheDocument();
    await user.click(screen.getByText('Slept well after studying'));
    expect(screen.getByText('1 of 6')).toBeInTheDocument();

    const raw = window.localStorage.getItem(HABIT_CHECKLIST_STORAGE_KEY);
    expect(raw).toBeTruthy();
    expect(JSON.parse(raw!)['sleep-consolidation']).toBe(true);

    await user.click(screen.getByText('Did some aerobic exercise'));
    expect(screen.getByText('2 of 6')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Reset/i }));
    expect(screen.getByText('0 of 6')).toBeInTheDocument();
    expect(window.localStorage.getItem(HABIT_CHECKLIST_STORAGE_KEY)).toBeNull();
  });

  it('opens the habits tab directly from the Study habits pill in StudyHabitsBar', async () => {
    const user = userEvent.setup();
    render(<StudyHabitsBar />);
    await user.click(screen.getByTitle('Open the study habits checklist'));
    expect(screen.getByText('Today’s study habits')).toBeInTheDocument();
  });

  it('renders completed habit indicators on the Study habits pill when items are checked', () => {
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

    expect(screen.getByText('Retrieval practice')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: /Habits/i }));
    expect(screen.getByText('Today’s study habits')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: /Techniques/i }));
    expect(screen.getByText('Retrieval practice')).toBeInTheDocument();
  });

  it('forwards onNavigate when a technique is chosen via the StudyHabitsBar guide', async () => {
    const user = userEvent.setup();
    const handleNavigate = vi.fn();
    render(<StudyHabitsBar onNavigate={handleNavigate} />);

    await user.click(screen.getByRole('button', { name: 'How to learn so it lasts' }));
    await user.click(screen.getByRole('button', { name: 'Open free recall' }));

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
