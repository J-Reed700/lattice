/**
 * Display Settings Tab
 *
 * Configure theme, appearance, and UI preferences.
 */

import { Palette, Sun, Moon, Monitor, Type, Eye, Layout } from 'lucide-react';

import { useSettingsStore, type DisplaySettings } from '../../stores/settingsStore';

import type { LucideIcon } from 'lucide-react';

export function DisplayTab() {
  const displaySettings = useSettingsStore((state) => state.settings.display);
  const updateDisplay = useSettingsStore((state) => state.updateDisplay);

  const themes: Array<{
    value: DisplaySettings['theme'];
    label: string;
    icon: LucideIcon;
    desc: string;
  }> = [
    {
      value: 'light',
      label: 'Light',
      icon: Sun,
      desc: 'Bright and clean',
    },
    {
      value: 'dark',
      label: 'Dark',
      icon: Moon,
      desc: 'Easy on the eyes',
    },
    {
      value: 'system',
      label: 'System',
      icon: Monitor,
      desc: 'Follow OS preference',
    },
  ];

  const fontSizes: Array<{
    value: DisplaySettings['fontSize'];
    label: string;
    example: string;
  }> = [
    { value: 'small', label: 'Small', example: '14px' },
    { value: 'medium', label: 'Medium', example: '16px' },
    { value: 'large', label: 'Large', example: '18px' },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Palette className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Display settings</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Customize the appearance and layout
          </p>
        </div>
      </div>

      {/* Theme Selection */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Palette className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Theme</h3>
        </div>
        <p className="text-xs text-[hsl(var(--text-secondary))]">
          Choose your preferred color scheme
        </p>

        <div className="grid grid-cols-3 gap-3">
          {themes.map((theme) => {
            const Icon = theme.icon;
            return (
              <button
                key={theme.value}
                onClick={() => updateDisplay({ theme: theme.value })}
                className={`
                  p-4 rounded-md border-2 transition-colors duration-fast
                  ${
                    displaySettings.theme === theme.value
                      ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]'
                      : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-default))] bg-[hsl(var(--surface-raised))]'
                  }
                `}
              >
                <Icon className="w-6 h-6 text-[hsl(var(--accent))] mx-auto mb-2" />
                <div className="text-sm font-medium text-[hsl(var(--text-primary))]">{theme.label}</div>
                <div className="text-xs text-[hsl(var(--text-secondary))] mt-1">{theme.desc}</div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Font Size */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Type className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Font Size</h3>
        </div>
        <p className="text-xs text-[hsl(var(--text-secondary))]">
          Adjust text size for better readability
        </p>

        <div className="grid grid-cols-3 gap-3">
          {fontSizes.map((size) => (
            <button
              key={size.value}
              onClick={() => updateDisplay({ fontSize: size.value })}
              className={`
                p-4 rounded-md border-2 transition-colors duration-fast
                ${
                  displaySettings.fontSize === size.value
                    ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]'
                    : 'border-[hsl(var(--border-subtle))] hover:border-[hsl(var(--border-default))] bg-[hsl(var(--surface-raised))]'
                }
              `}
            >
              <div className="text-sm font-medium text-[hsl(var(--text-primary))] mb-1">
                {size.label}
              </div>
              <div className="text-xs text-[hsl(var(--text-tertiary))] font-mono">{size.example}</div>
            </button>
          ))}
        </div>
      </div>

      {/* Layout Options */}
      <div className="space-y-4 p-4 bg-[hsl(var(--surface))] rounded-lg">
        <div className="flex items-center gap-2">
          <Layout className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Layout Options</h3>
        </div>

        {/* Compact Mode */}
        <div className="flex items-start gap-3">
          <input
            id="compactMode"
            type="checkbox"
            checked={displaySettings.compactMode}
            onChange={(e) => updateDisplay({ compactMode: e.target.checked })}
            className="mt-1 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface-raised))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
          />
          <div className="flex-1">
            <label
              htmlFor="compactMode"
              className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer"
            >
              Compact Mode
            </label>
            <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
              Reduce spacing and padding for a denser layout. Good for smaller screens.
            </p>
          </div>
        </div>

        {/* Show Previews */}
        <div className="flex items-start gap-3">
          <input
            id="showPreviews"
            type="checkbox"
            checked={displaySettings.showPreviews}
            onChange={(e) => updateDisplay({ showPreviews: e.target.checked })}
            className="mt-1 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface-raised))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
          />
          <div className="flex-1">
            <label
              htmlFor="showPreviews"
              className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer"
            >
              Show File Previews
            </label>
            <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
              Display thumbnail previews and content snippets in search results and file lists.
            </p>
          </div>
        </div>
      </div>

      {/* Preview Section */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Eye className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Preview</h3>
        </div>

        <div
          className={`
            p-6 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]
            ${displaySettings.compactMode ? 'space-y-2' : 'space-y-4'}
          `}
        >
          <div
            className={`
              ${
                displaySettings.fontSize === 'small'
                  ? 'text-sm'
                  : displaySettings.fontSize === 'large'
                  ? 'text-lg'
                  : 'text-base'
              }
            `}
          >
            <h4 className="font-semibold text-[hsl(var(--text-primary))]">Sample Heading</h4>
            <p className="text-[hsl(var(--text-secondary))] mt-2">
              This is how text will appear with your current settings. The quick brown fox jumps
              over the lazy dog.
            </p>
          </div>

          {displaySettings.showPreviews && (
            <div className="mt-4 p-3 bg-[hsl(var(--surface))] rounded border border-[hsl(var(--border-subtle))]">
              <div className="text-xs text-[hsl(var(--text-tertiary))] mb-1">Preview enabled</div>
              <div
                className={`
                  text-[hsl(var(--text-secondary))]
                  ${
                    displaySettings.fontSize === 'small'
                      ? 'text-xs'
                      : displaySettings.fontSize === 'large'
                      ? 'text-sm'
                      : 'text-xs'
                  }
                `}
              >
                File content preview would appear here...
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Accessibility Note */}
      <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
        <div className="flex gap-3">
          <div className="flex-shrink-0">
            <svg
              className="w-5 h-5 text-[hsl(var(--accent))]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
          <div className="flex-1">
            <h4 className="text-sm font-medium text-[hsl(var(--text-primary))]">Accessibility</h4>
            <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
              All themes meet WCAG AA contrast standards. The app respects your system's reduced
              motion preferences.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
