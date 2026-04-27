/**
 * Header Component
 *
 * Purpose: Consistent top bar across all application views
 *
 * Features:
 * - View title and context
 * - Quick search access
 * - Theme toggle
 * - Help button
 * - Breadcrumb navigation (optional)
 *
 * States: Default only
 * Accessibility: Semantic HTML, ARIA landmarks, keyboard navigation
 */

import { Search, HelpCircle, Moon, Sun } from 'lucide-react';

import { useEffectiveTheme } from '../../hooks/useApplyTheme';
import { useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';

export interface HeaderProps {
  /** Current view title */
  title?: string;

  /** Optional subtitle/context */
  subtitle?: string;

  /** Show quick search button */
  showSearch?: boolean;

  /** Callback when search clicked */
  onSearchClick?: () => void;

  /** Show help button */
  showHelp?: boolean;

  /** Callback when help clicked */
  onHelpClick?: () => void;

  /** Custom actions to display */
  actions?: React.ReactNode;

  /** Breadcrumb items */
  breadcrumbs?: Array<{
    label: string;
    onClick?: () => void;
  }>;

  /** Additional CSS classes */
  className?: string;
}

export function Header({
  title,
  subtitle,
  showSearch = true,
  onSearchClick,
  showHelp = true,
  onHelpClick,
  actions,
  breadcrumbs,
  className = '',
}: HeaderProps) {
  const updateSettings = useUpdateSettingsMutation();
  const effectiveTheme = useEffectiveTheme();

  const toggleTheme = () => {
    const newTheme = effectiveTheme === 'dark' ? 'light' : 'dark';
    updateSettings.mutate({ category: 'ui', updates: { theme: newTheme } });
  };

  return (
    <header
      className={`flex items-center justify-between px-6 py-4 bg-[hsl(var(--surface))] border-b border-[hsl(var(--border-subtle))] ${className}`}
      role="banner"
    >
      {/* Left: Title and Breadcrumbs */}
      <div className="flex-1 min-w-0">
        {breadcrumbs && breadcrumbs.length > 0 ? (
          <nav aria-label="Breadcrumb">
            <ol className="flex items-center gap-2 text-sm mb-1">
              {breadcrumbs.map((crumb, index) => (
                <li key={index} className="flex items-center gap-2">
                  {index > 0 && (
                    <span className="text-[hsl(var(--text-tertiary))]">/</span>
                  )}
                  {crumb.onClick ? (
                    <button
                      onClick={crumb.onClick}
                      className="text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                    >
                      {crumb.label}
                    </button>
                  ) : (
                    <span className="text-[hsl(var(--text-primary))] font-medium">
                      {crumb.label}
                    </span>
                  )}
                </li>
              ))}
            </ol>
          </nav>
        ) : null}

        {title && (
          <div>
            <h1 className="text-xl font-semibold text-[hsl(var(--text-primary))] truncate">
              {title}
            </h1>
            {subtitle && (
              <p className="text-sm text-[hsl(var(--text-secondary))] truncate mt-1">
                {subtitle}
              </p>
            )}
          </div>
        )}
      </div>

      {/* Right: Actions */}
      <div className="flex items-center gap-2 ml-4">
        {/* Custom actions */}
        {actions}

        {/* Quick search */}
        {showSearch && (
          <button
            onClick={onSearchClick}
            className="p-2 rounded-md text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            title="Search (⌘K)"
            aria-label="Open search"
          >
            <Search className="w-4 h-4" strokeWidth={1.75} />
          </button>
        )}

        {/* Theme toggle */}
        <button
          onClick={toggleTheme}
          className="p-2 rounded-md text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          title={`Switch to ${effectiveTheme === 'dark' ? 'light' : 'dark'} mode`}
          aria-label={`Switch to ${effectiveTheme === 'dark' ? 'light' : 'dark'} mode`}
        >
          {effectiveTheme === 'dark' ? (
            <Sun className="w-4 h-4" strokeWidth={1.75} />
          ) : (
            <Moon className="w-4 h-4" strokeWidth={1.75} />
          )}
        </button>

        {/* Help */}
        {showHelp && (
          <button
            onClick={onHelpClick}
            className="p-2 rounded-md text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            title="Help (?)"
            aria-label="Open help"
          >
            <HelpCircle className="w-4 h-4" strokeWidth={1.75} />
          </button>
        )}
      </div>
    </header>
  );
}

/**
 * Compact Header - Minimal version for constrained spaces
 */
export function CompactHeader({
  title,
  onBack,
  actions,
}: {
  title: string;
  onBack?: () => void;
  actions?: React.ReactNode;
}) {
  return (
    <header className="flex items-center justify-between px-4 py-3 bg-[hsl(var(--surface))] border-b border-[hsl(var(--border-subtle))]">
      <div className="flex items-center gap-3 flex-1 min-w-0">
        {onBack && (
          <button
            onClick={onBack}
            className="p-1 rounded-md text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast"
            aria-label="Go back"
          >
            <svg
              className="w-4 h-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.75}
                d="M15 19l-7-7 7-7"
              />
            </svg>
          </button>
        )}
        <h2 className="text-lg font-semibold text-[hsl(var(--text-primary))] truncate">
          {title}
        </h2>
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  );
}
