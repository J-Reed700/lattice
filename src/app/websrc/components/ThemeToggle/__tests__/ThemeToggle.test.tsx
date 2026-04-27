import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { useSettingsStore } from '../../../stores/settingsStore';
import { ThemeToggle } from '../ThemeToggle';

vi.mock('../../../stores/settingsStore');
vi.mock('../../ui', () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

describe('ThemeToggle', () => {
  const mockUpdateDisplay = vi.fn();
  let user: ReturnType<typeof userEvent.setup>;

  beforeEach(() => {
    user = userEvent.setup();
    vi.clearAllMocks();

    vi.mocked(useSettingsStore).mockImplementation((selector: any) => {
      const state = {
        settings: {
          display: {
            theme: 'system',
          },
        },
        updateDisplay: mockUpdateDisplay,
      };
      return selector(state);
    });
  });

  describe('Rendering', () => {
    it('renders all three theme options', () => {
      render(<ThemeToggle />);

      expect(screen.getByLabelText('Switch to light theme')).toBeInTheDocument();
      expect(screen.getByLabelText('Switch to dark theme')).toBeInTheDocument();
      expect(screen.getByLabelText('Switch to system theme')).toBeInTheDocument();
    });

    it('renders theme icons', () => {
      const { container } = render(<ThemeToggle />);

      const svgs = container.querySelectorAll('svg');
      expect(svgs).toHaveLength(3);
    });

    it('renders in a container with border', () => {
      const { container } = render(<ThemeToggle />);

      const themeToggle = container.querySelector('.border-dashed, .border');
      expect(themeToggle).toHaveClass('border');
    });
  });

  describe('Theme Selection', () => {
    it('highlights system theme by default', () => {
      render(<ThemeToggle />);

      const systemButton = screen.getByLabelText('Switch to system theme');
      expect(systemButton).toHaveClass('bg-[var(--accent-primary)]');
      expect(systemButton).toHaveClass('text-white');
    });

    it('highlights light theme when selected', () => {
      vi.mocked(useSettingsStore).mockImplementation((selector: any) => {
        const state = {
          settings: {
            display: {
              theme: 'light',
            },
          },
          updateDisplay: mockUpdateDisplay,
        };
        return selector(state);
      });

      render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      expect(lightButton).toHaveClass('bg-[var(--accent-primary)]');
    });

    it('highlights dark theme when selected', () => {
      vi.mocked(useSettingsStore).mockImplementation((selector: any) => {
        const state = {
          settings: {
            display: {
              theme: 'dark',
            },
          },
          updateDisplay: mockUpdateDisplay,
        };
        return selector(state);
      });

      render(<ThemeToggle />);

      const darkButton = screen.getByLabelText('Switch to dark theme');
      expect(darkButton).toHaveClass('bg-[var(--accent-primary)]');
    });
  });

  describe('Theme Switching', () => {
    it('switches to light theme on click', async () => {
      render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      await user.click(lightButton);

      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'light' });
    });

    it('switches to dark theme on click', async () => {
      render(<ThemeToggle />);

      const darkButton = screen.getByLabelText('Switch to dark theme');
      await user.click(darkButton);

      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'dark' });
    });

    it('switches to system theme on click', async () => {
      vi.mocked(useSettingsStore).mockImplementation((selector: any) => {
        const state = {
          settings: {
            display: {
              theme: 'light',
            },
          },
          updateDisplay: mockUpdateDisplay,
        };
        return selector(state);
      });

      render(<ThemeToggle />);

      const systemButton = screen.getByLabelText('Switch to system theme');
      await user.click(systemButton);

      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'system' });
    });

    it('can switch between all themes', async () => {
      const { rerender } = render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      await user.click(lightButton);
      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'light' });

      mockUpdateDisplay.mockClear();

      vi.mocked(useSettingsStore).mockImplementation((selector: any) => {
        const state = {
          settings: {
            display: {
              theme: 'light',
            },
          },
          updateDisplay: mockUpdateDisplay,
        };
        return selector(state);
      });

      rerender(<ThemeToggle />);

      const darkButton = screen.getByLabelText('Switch to dark theme');
      await user.click(darkButton);
      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'dark' });
    });
  });

  describe('Visual States', () => {
    it('applies correct styles to active button', () => {
      render(<ThemeToggle />);

      const systemButton = screen.getByLabelText('Switch to system theme');
      expect(systemButton).toHaveClass('bg-[var(--accent-primary)]');
      expect(systemButton).toHaveClass('text-white');
      expect(systemButton).toHaveClass('shadow-sm');
    });

    it('applies correct styles to inactive buttons', () => {
      render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      expect(lightButton).toHaveClass('bg-transparent');
      expect(lightButton).toHaveClass('text-[var(--text-secondary)]');
    });

    it('shows hover styles on inactive buttons', () => {
      render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      expect(lightButton).toHaveClass('hover:bg-[var(--surface-hover)]');
      expect(lightButton).toHaveClass('hover:text-[var(--text-primary)]');
    });
  });

  describe('Tooltips', () => {
    it('shows tooltip for light theme', () => {
      render(<ThemeToggle />);

      expect(screen.getByText('Switch to light theme')).toBeInTheDocument();
    });

    it('shows tooltip for dark theme', () => {
      render(<ThemeToggle />);

      expect(screen.getByText('Switch to dark theme')).toBeInTheDocument();
    });

    it('shows tooltip for system theme', () => {
      render(<ThemeToggle />);

      expect(screen.getByText('Switch to system theme')).toBeInTheDocument();
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA labels', () => {
      render(<ThemeToggle />);

      expect(screen.getByLabelText('Switch to light theme')).toHaveAccessibleName();
      expect(screen.getByLabelText('Switch to dark theme')).toHaveAccessibleName();
      expect(screen.getByLabelText('Switch to system theme')).toHaveAccessibleName();
    });

    it('buttons are keyboard accessible', async () => {
      render(<ThemeToggle />);

      const lightButton = screen.getByLabelText('Switch to light theme');
      lightButton.focus();

      expect(lightButton).toHaveFocus();

      await user.keyboard('{Enter}');

      expect(mockUpdateDisplay).toHaveBeenCalledWith({ theme: 'light' });
    });

    it('buttons have proper button role', () => {
      render(<ThemeToggle />);

      const buttons = screen.getAllByRole('button');
      expect(buttons).toHaveLength(3);
    });
  });

  describe('Button Sizing', () => {
    it('has consistent button sizes', () => {
      const { container } = render(<ThemeToggle />);

      const buttons = container.querySelectorAll('button');
      buttons.forEach((button) => {
        expect(button).toHaveClass('w-9');
        expect(button).toHaveClass('h-9');
      });
    });

    it('has consistent icon sizes', () => {
      const { container } = render(<ThemeToggle />);

      const svgs = container.querySelectorAll('svg');
      svgs.forEach((svg) => {
        expect(svg).toHaveClass('w-[18px]');
        expect(svg).toHaveClass('h-[18px]');
      });
    });
  });

  describe('Transitions', () => {
    it('applies transition classes to buttons', () => {
      const { container } = render(<ThemeToggle />);

      const buttons = container.querySelectorAll('button');
      buttons.forEach((button) => {
        expect(button).toHaveClass('transition-all');
        expect(button).toHaveClass('duration-200');
      });
    });
  });
});
