import { useEffect, useState } from 'react';

import {
  Activity,
  AlertTriangle,
  Clock3,
  Coffee,
  GraduationCap,
  Heart,
  Moon,
  Pause,
  Play,
  RotateCcw,
  ShieldAlert,
  ShieldCheck,
} from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';

import { RetentionFrameworksModal } from './RetentionFrameworksModal';
import { useHabitChecklist } from './useHabitChecklist';

import type { FrameworkAction } from './types';

export type { FrameworkAction };

export interface StudyHabitsBarProps {
  onSelectFramework?: (action: FrameworkAction) => void;
  onNavigate?: (action: FrameworkAction) => void;
  className?: string;
  /** Compact inline mode for the 2-tier ProgramWorkspace header. */
  compact?: boolean;
}

type PresetKey = '30m' | '45m' | '25m' | '5m-break';

interface PresetConfig {
  key: PresetKey;
  label: string;
  durationSeconds: number;
  isBreak?: boolean;
}

const PRESETS: readonly PresetConfig[] = [
  { key: '30m', label: '30 min', durationSeconds: 30 * 60 },
  { key: '45m', label: '45 min', durationSeconds: 45 * 60 },
  { key: '25m', label: '25 min', durationSeconds: 25 * 60 },
  { key: '5m-break', label: '5 min Break', durationSeconds: 5 * 60, isBreak: true },
] as const;

function formatSeconds(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

export function StudyHabitsBar({ onSelectFramework, onNavigate, className, compact = false }: StudyHabitsBarProps) {
  const [activePreset, setActivePreset] = useState<PresetKey>('30m');
  const [remainingSeconds, setRemainingSeconds] = useState<number>(30 * 60);
  const [isRunning, setIsRunning] = useState<boolean>(false);
  const [cumulativeStudySeconds, setCumulativeStudySeconds] = useState<number>(0);
  const [isCrammingAlertDismissed, setIsCrammingAlertDismissed] = useState<boolean>(false);
  const [isFocusGuardEnabled, setIsFocusGuardEnabled] = useState<boolean>(false);

  // Modal control
  const [isModalOpen, setIsModalOpen] = useState<boolean>(false);
  const [modalTab, setModalTab] = useState<'frameworks' | 'habits'>('frameworks');

  const { checklist, completedCount, totalCount } = useHabitChecklist();

  // Timer interval effect
  useEffect(() => {
    if (!isRunning) return;

    const interval = window.setInterval(() => {
      setRemainingSeconds((prev) => {
        if (prev <= 1) {
          setIsRunning(false);
          return 0;
        }
        return prev - 1;
      });

      // Track study time if not on a break
      const currentConfig = PRESETS.find((p) => p.key === activePreset);
      if (!currentConfig?.isBreak) {
        setCumulativeStudySeconds((prev) => prev + 1);
      }
    }, 1000);

    return () => window.clearInterval(interval);
  }, [isRunning, activePreset]);

  const handleSelectPreset = (preset: PresetKey) => {
    const config = PRESETS.find((p) => p.key === preset);
    if (!config) return;
    setActivePreset(preset);
    setRemainingSeconds(config.durationSeconds);
    setIsRunning(false);
    if (config.isBreak) {
      // Taking a break resets the continuous study counter and dismisses alert
      setCumulativeStudySeconds(0);
      setIsCrammingAlertDismissed(false);
    }
  };

  const handleStartPause = () => {
    if (remainingSeconds === 0) {
      const config = PRESETS.find((p) => p.key === activePreset);
      setRemainingSeconds(config?.durationSeconds ?? 30 * 60);
    }
    setIsRunning((prev) => !prev);
  };

  const handleReset = () => {
    setIsRunning(false);
    const config = PRESETS.find((p) => p.key === activePreset);
    setRemainingSeconds(config?.durationSeconds ?? 30 * 60);
  };

  const handleTakeBreak = () => {
    setActivePreset('5m-break');
    setRemainingSeconds(5 * 60);
    setCumulativeStudySeconds(0);
    setIsCrammingAlertDismissed(false);
    setIsRunning(true);
  };

  const openFrameworksModal = (tab: 'frameworks' | 'habits' = 'frameworks') => {
    setModalTab(tab);
    setIsModalOpen(true);
  };

  // Continuous study time in minutes
  const continuousStudyMinutes = Math.floor(cumulativeStudySeconds / 60);
  const showCrammingAlert = continuousStudyMinutes >= 90 && !isCrammingAlertDismissed;

  const sleepDone = Boolean(checklist['sleep-consolidation']);
  const movementDone = Boolean(checklist['aerobic-movement']);
  const focusDone = Boolean(checklist['focused-block']);

  if (compact) {
    return (
      <div className={cn('inline-flex items-center gap-1', className)}>
        <Popover>
          <PopoverTrigger asChild>
            <Button
              size="sm"
              variant="ghost"
              className={cn(
                'h-8 gap-1.5 px-2 text-xs font-medium',
                isRunning || isFocusGuardEnabled || showCrammingAlert
                  ? 'text-accent'
                  : 'text-text-secondary hover:text-text-primary',
              )}
              title="Focus timer & study habits"
              aria-label="Focus timer and study habits"
            >
              <Clock3 className="size-3.5 shrink-0" aria-hidden="true" />
              <span className="font-mono text-xs tabular-nums">{formatSeconds(remainingSeconds)}</span>
              {isRunning && <span className="size-1.5 rounded-full bg-accent" aria-hidden="true" />}
              {showCrammingAlert && <AlertTriangle className="size-3.5 text-warning-fg" aria-hidden="true" />}
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-80 space-y-3 p-3.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-text-primary">Focus timer</span>
              <span className="font-mono text-sm font-semibold tabular-nums text-text-primary" aria-live="polite">
                {formatSeconds(remainingSeconds)}
              </span>
            </div>
            <div className="grid grid-cols-4 gap-1 rounded-lg border border-border-subtle bg-surface-sunken p-0.5">
              {PRESETS.map((preset) => {
                const isSelected = activePreset === preset.key;
                return (
                  <button
                    key={preset.key}
                    type="button"
                    onClick={() => handleSelectPreset(preset.key)}
                    className={cn(
                      'rounded-md px-1.5 py-1 text-[11px] font-medium transition-colors',
                      isSelected
                        ? 'bg-surface text-text-primary shadow-control'
                        : 'text-text-muted hover:text-text-primary',
                    )}
                  >
                    {preset.label.replace(' Break', '')}
                  </button>
                );
              })}
            </div>
            <div className="flex items-center gap-1.5">
              <Button
                size="sm"
                variant={isRunning ? 'secondary' : 'default'}
                className="flex-1"
                onClick={handleStartPause}
                aria-label={isRunning ? 'Pause focus timer' : 'Start focus timer'}
              >
                {isRunning ? <><Pause className="size-3" /> Pause</> : <><Play className="size-3" /> Start</>}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={handleReset}
                aria-label="Reset timer"
                title="Reset focus timer"
              >
                <RotateCcw className="size-3" /> Reset
              </Button>
            </div>
            {showCrammingAlert && (
              <div role="alert" className="rounded-lg border border-warning/30 bg-warning-muted/40 p-2.5 text-xs text-warning-fg">
                <p className="font-semibold">Time for a break ({continuousStudyMinutes}m continuous)</p>
                <div className="mt-2 flex gap-1.5">
                  <Button size="sm" variant="outline" onClick={handleTakeBreak}>
                    <Coffee className="size-3" /> 5-min Break
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setIsCrammingAlertDismissed(true)}>
                    Dismiss
                  </Button>
                </div>
              </div>
            )}
            <div className="flex items-center justify-between gap-2 border-t border-border-subtle pt-2.5">
              <button
                type="button"
                onClick={() => setIsFocusGuardEnabled((prev) => !prev)}
                className={cn(
                  'inline-flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs font-medium transition-colors',
                  isFocusGuardEnabled
                    ? 'border-success/30 bg-success-muted text-success-fg'
                    : 'border-border-subtle bg-surface text-text-secondary hover:border-border-default hover:text-text-primary',
                )}
              >
                {isFocusGuardEnabled ? <ShieldCheck className="size-3 text-success-fg" /> : <ShieldAlert className="size-3 text-text-muted" />}
                Focus guard
              </button>
              <button
                type="button"
                onClick={() => openFrameworksModal('habits')}
                className="inline-flex h-7 items-center gap-1.5 rounded-md border border-border-subtle bg-surface px-2 text-xs text-text-secondary transition-colors hover:border-border-default hover:text-text-primary"
              >
                <Heart className="size-3 text-accent" />
                <span>Habits</span>
                <span className="font-mono text-[10px] text-text-muted">{completedCount}/{totalCount}</span>
              </button>
            </div>
            {isFocusGuardEnabled && (
              <p className="rounded-md bg-success-muted/35 px-2.5 py-1.5 text-[11px] leading-relaxed text-success-fg">
                Single-tasking active: one tab, no notifications, 30–45m focus block.
              </p>
            )}
          </PopoverContent>
        </Popover>

        <Button
          size="sm"
          variant="ghost"
          className="h-8 gap-1.5 px-2 text-xs font-medium text-text-secondary hover:text-text-primary"
          onClick={() => openFrameworksModal('frameworks')}
          aria-label="How to learn so it lasts"
          title="How to learn so it lasts"
        >
          <GraduationCap className="size-3.5 text-accent" aria-hidden="true" />
          <span className="hidden xl:inline">Guide</span>
        </Button>

        <RetentionFrameworksModal
          open={isModalOpen}
          onOpenChange={setIsModalOpen}
          initialTab={modalTab}
          onSelectFramework={onSelectFramework}
          onNavigate={onNavigate}
        />
      </div>
    );
  }

  return (
    <div className={cn('space-y-2', className)}>
      {/* Anti-Cramming & Single-Tasking Guard Alert (when study >= 90 continuous minutes) */}
      {showCrammingAlert && (
        <div
          role="alert"
          className="flex items-center justify-between gap-3 rounded-lg border border-warning-muted bg-warning-muted/30 px-3.5 py-2 text-xs text-warning-fg"
        >
          <div className="flex items-center gap-2">
            <AlertTriangle className="size-4 shrink-0 text-warning-fg" />
            <div>
              <span className="font-semibold text-warning-fg">
                Time for a break:
              </span>{' '}
              You have been studying for {continuousStudyMinutes} minutes without one.
              A short break helps you stay focused, and coming back tomorrow helps you remember more than pressing on today.
            </div>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Button
              size="sm"
              variant="outline"
              className="h-6 text-xs font-medium"
              onClick={handleTakeBreak}
            >
              <Coffee className="size-3 mr-1" />
              Take 5-min Break
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-6 text-xs text-warning-fg hover:bg-warning-muted/60"
              onClick={() => setIsCrammingAlertDismissed(true)}
            >
              Dismiss
            </Button>
          </div>
        </div>
      )}

      {/* Main Habits Bar Container — sleek compact utility strip */}
      <div className="studio-habits-controls flex flex-wrap items-center justify-between gap-2.5 rounded-xl border border-border-default bg-surface/90 px-3.5 py-2 shadow-2xs backdrop-blur-xs">
        {/* Left: Pomodoro Focus Timer Segmented Control */}
        <div className="flex flex-wrap items-center gap-2">
          <div className="studio-timer inline-flex items-center gap-1.5 rounded-lg border border-border-subtle bg-surface-sunken p-0.5">
            {/* Presets Segmented Pill */}
            <div className="inline-flex items-center gap-0.5">
              {PRESETS.map((preset) => {
                const isSelected = activePreset === preset.key;
                return (
                  <button
                    key={preset.key}
                    type="button"
                    onClick={() => handleSelectPreset(preset.key)}
                    className={cn(
                      'rounded-md px-2 py-0.5 text-xs font-medium transition-colors',
                      isSelected
                        ? 'bg-surface text-text-primary shadow-2xs'
                        : 'text-text-muted hover:text-text-primary hover:bg-surface-raised/60'
                    )}
                  >
                    {preset.label}
                  </button>
                );
              })}
            </div>

            <div className="h-3.5 w-px bg-border-subtle" />

            {/* Time Readout */}
            <div
              className="font-mono text-xs font-semibold tabular-nums text-text-primary px-1.5 py-0.5"
              aria-live="polite"
            >
              {formatSeconds(remainingSeconds)}
            </div>

            {/* Start / Pause / Reset Controls */}
            <div className="flex items-center gap-0.5">
              <Button
                size="sm"
                variant={isRunning ? 'secondary' : 'default'}
                className="h-6 px-2 text-xs font-medium"
                onClick={handleStartPause}
                aria-label={isRunning ? 'Pause focus timer' : 'Start focus timer'}
              >
                {isRunning ? (
                  <>
                    <Pause className="size-3 mr-1" />
                    Pause
                  </>
                ) : (
                  <>
                    <Play className="size-3 mr-1" />
                    Start
                  </>
                )}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="h-6 px-1.5 text-xs text-text-muted hover:text-text-primary hover:bg-surface-raised"
                onClick={handleReset}
                aria-label="Reset timer"
                title="Reset focus timer"
              >
                <RotateCcw className="size-3 mr-1" />
                Reset
              </Button>
            </div>
          </div>

          {/* Continuous Study Time Indicator */}
          {continuousStudyMinutes > 0 && (
            <span className="text-[11px] text-text-muted hidden md:inline">
              Session: {continuousStudyMinutes}m
            </span>
          )}
        </div>

        {/* Right: dividers, focus guard, study habits, learning guide */}
        <div className="flex flex-wrap items-center gap-2">
          {/* Single-Tasking & Focus Guard Toggle */}
          <button
            type="button"
            onClick={() => setIsFocusGuardEnabled((prev) => !prev)}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium transition-colors',
              isFocusGuardEnabled
                ? 'border-success-muted bg-success-muted text-success-fg'
                : 'border-border-subtle bg-surface text-text-secondary hover:border-border-default hover:text-text-primary'
            )}
            title="Toggle focus guard: avoid multitasking & cramming"
          >
            {isFocusGuardEnabled ? (
              <ShieldCheck className="size-3 text-success-fg" />
            ) : (
              <ShieldAlert className="size-3 text-text-muted" />
            )}
            <span>Focus guard</span>
          </button>

          <div className="h-4 w-px bg-border-subtle hidden sm:block" />

          {/* Health & Consolidation Pill */}
          <button
            type="button"
            onClick={() => openFrameworksModal('habits')}
            className="group inline-flex items-center gap-1.5 rounded-lg border border-border-subtle bg-surface px-2.5 py-1 text-xs text-text-secondary hover:border-border-default hover:text-text-primary transition-colors"
            title="Open the study habits checklist"
          >
            <Heart className="size-3 text-accent group-hover:scale-110 transition-transform" />
            <span className="font-medium">Study habits</span>
            <span className="inline-flex items-center gap-1 text-[10px] text-text-muted ml-0.5">
              <Moon
                className={cn('size-2.5', sleepDone ? 'text-blue-500 font-bold' : 'opacity-40')}
                aria-label={sleepDone ? 'Sleep completed' : 'Sleep pending'}
              />
              <Activity
                className={cn('size-2.5', movementDone ? 'text-emerald-500 font-bold' : 'opacity-40')}
                aria-label={movementDone ? 'Aerobic movement completed' : 'Movement pending'}
              />
              <Coffee
                className={cn('size-2.5', focusDone ? 'text-amber-500 font-bold' : 'opacity-40')}
                aria-label={focusDone ? 'Focused block completed' : 'Focused block pending'}
              />
            </span>
            <Badge variant="secondary" className="h-4 px-1 text-[10px] ml-0.5 bg-surface-raised text-text-secondary border-border-subtle">
              {completedCount}/{totalCount}
            </Badge>
          </button>

          <div className="h-4 w-px bg-border-subtle hidden sm:block" />

          {/* Learning guide button */}
          <Button
            size="sm"
            variant="outline"
            className="h-7 gap-1.5 text-xs font-medium border-border-subtle bg-surface text-text-secondary hover:border-border-default hover:text-text-primary"
            onClick={() => openFrameworksModal('frameworks')}
            aria-label="How to learn so it lasts"
          >
            <GraduationCap className="size-3.5 text-accent" />
            <span>How to learn</span>
          </Button>
        </div>
      </div>

      {/* Focus Guard Reminder Banner (when enabled) */}
      {isFocusGuardEnabled && (
        <div className="flex items-center gap-2 rounded-lg border border-success-muted bg-success-muted/30 px-3 py-1.5 text-xs text-success-fg">
          <ShieldCheck className="size-3.5 shrink-0 text-success-fg" />
          <span>
            Single-tasking active: one tab, no phone or notifications, 30–45 minute undistracted focus block.
          </span>
        </div>
      )}

      {/* Learning guide and habits modal */}
      <RetentionFrameworksModal
        open={isModalOpen}
        onOpenChange={setIsModalOpen}
        initialTab={modalTab}
        onSelectFramework={onSelectFramework}
        onNavigate={onNavigate}
      />
    </div>
  );
}
