import { ArrowLeft, ShieldCheck } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import { HELP_NAMES, SETTING_NAMES } from './practiceNames';
import { ElapsedClock } from '../shared/ElapsedClock';

const SETTINGS: Wire.Setting[] = ['coached', 'mock', 'watched'];

export interface SessionHeaderProps {
  session: Wire.PracticeSessionDto;
  clock: Wire.ClockDto;
  elapsedMs: number;
  finishing: boolean;
  onExit: () => void;
  onChangeSetting: (setting: Wire.Setting) => void;
  onFinish: () => void;
}

export function SessionHeader({ session, clock, elapsedMs, finishing, onExit, onChangeSetting, onFinish }: SessionHeaderProps) {
  const { setting } = session;

  return (
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={onExit}
          className="inline-flex min-h-[44px] cursor-pointer items-center gap-1.5 rounded-lg px-2 text-sm font-medium text-text-secondary hover:text-text-primary"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          <span>Practice</span>
        </button>
        <h1 className="m-0 font-serif text-xl font-semibold text-text-primary">{session.title}</h1>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-2">
          <label htmlFor="practice-setting" className="text-sm text-text-secondary">
            Setting
          </label>
          <select
            id="practice-setting"
            value={setting}
            onChange={(event) => onChangeSetting(event.target.value as Wire.Setting)}
            className="min-h-[44px] rounded-lg border border-border-default bg-surface px-2 text-sm text-text-primary"
          >
            {SETTINGS.map((option) => (
              <option key={option} value={option}>
                {SETTING_NAMES[option]}
              </option>
            ))}
          </select>
          <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-1 text-xs font-medium text-text-secondary">
            <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{HELP_NAMES[setting]}</span>
          </span>
        </div>
        <ElapsedClock
          elapsedMs={elapsedMs}
          running={clock.openSince != null && !session.finished}
          softLimitMs={clock.budgetSeconds != null ? clock.budgetSeconds * 1000 : null}
          label="Elapsed"
        />
        <button
          type="button"
          onClick={onFinish}
          disabled={finishing}
          className="min-h-[44px] cursor-pointer rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Finish
        </button>
      </div>
    </header>
  );
}
