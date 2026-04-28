import React from 'react';

import { useSettingsStore } from '../../stores/settingsStore';
import { Tooltip, TooltipTrigger, TooltipContent } from '../ui';

type Theme = 'light' | 'dark' | 'system';

interface ThemeOption {
  value: Theme;
  label: string;
  icon: React.ReactNode;
}

export function ThemeToggle() {
  const theme = useSettingsStore((state) => state.display.theme);
  const updateDisplay = useSettingsStore((state) => state.updateDisplay);

  const options: ThemeOption[] = [
    {
      value: 'light',
      label: 'Light',
      icon: (
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.75}
            d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"
          />
        </svg>
      ),
    },
    {
      value: 'dark',
      label: 'Dark',
      icon: (
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.75}
            d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"
          />
        </svg>
      ),
    },
    {
      value: 'system',
      label: 'System',
      icon: (
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.75}
            d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
          />
        </svg>
      ),
    },
  ];

  return (
    <div className="flex gap-1 p-1 bg-[hsl(var(--bg))] rounded-lg border border-[hsl(var(--border-subtle))]">
      {options.map((option) => (
        <Tooltip key={option.value}>
          <TooltipTrigger asChild>
            <button
              onClick={() => updateDisplay({ theme: option.value })}
              className={`
                flex items-center justify-center
                w-9 h-9
                rounded-md
                border-none
                transition-colors duration-fast
                ${
                  theme === option.value
                    ? 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] shadow-sm'
                    : 'bg-transparent text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
                }
              `}
              aria-label={`Switch to ${option.label.toLowerCase()} theme`}
            >
              {option.icon}
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {`Switch to ${option.label.toLowerCase()} theme`}
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
}
