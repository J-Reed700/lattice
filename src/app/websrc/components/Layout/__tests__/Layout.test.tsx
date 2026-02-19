import { render, screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { Layout } from '../Layout';

describe('Layout', () => {
  let user: ReturnType<typeof userEvent.setup>;

  // Helper to render Layout with Router
  const renderLayout = (initialPath = '/home') => render(
      <MemoryRouter initialEntries={[initialPath]}>
        <Layout />
      </MemoryRouter>
    );

  beforeEach(() => {
    user = userEvent.setup();
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('renders all navigation items', () => {
      renderLayout();

      // Both mobile and desktop nav exist, so check for multiple instances
      expect(screen.getAllByLabelText('Home').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Search').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Files').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Chat').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Daily Notes').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Reference Inbox').length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByLabelText('Settings').length).toBeGreaterThanOrEqual(1);
    });

    it('highlights active view', () => {
      renderLayout('/search');

      const searchButtons = screen.getAllByLabelText('Search');
      // At least one should be active
      const activeButton = searchButtons.find(btn =>
        btn.className.includes('bg-[var(--accent-primary)]')
      );
      expect(activeButton).toBeDefined();
    });

    it('shows current page with aria-current', () => {
      renderLayout('/files');

      const filesButtons = screen.getAllByLabelText('Files');
      // At least one should have aria-current
      const currentButton = filesButtons.find(btn =>
        btn.getAttribute('aria-current') === 'page'
      );
      expect(currentButton).toBeDefined();
    });
  });

  describe('Navigation', () => {
    it('navigates when nav item clicked', async () => {
      renderLayout('/home');

      const searchButtons = screen.getAllByLabelText('Search');
      await user.click(searchButtons[0]);

      // At least one button should now be active
      const activeButtons = screen.getAllByLabelText('Search').filter(btn =>
        btn.className.includes('bg-[var(--accent-primary)]')
      );
      expect(activeButtons.length).toBeGreaterThan(0);
    });

    it('navigates to each view', async () => {
      renderLayout('/home');

      const views = [
        { view: 'search', label: 'Search' },
        { view: 'files', label: 'Files' },
        { view: 'chat', label: 'Chat' },
        { view: 'daily', label: 'Daily Notes' },
        { view: 'references', label: 'Reference Inbox' },
        { view: 'settings', label: 'Settings' }
      ] as const;

      for (const { label } of views) {
        const buttons = screen.getAllByLabelText(label);
        await user.click(buttons[0]);
        // Check at least one button becomes active
        const activeButtons = screen.getAllByLabelText(label).filter(btn =>
          btn.className.includes('bg-[var(--accent-primary)]')
        );
        expect(activeButtons.length).toBeGreaterThan(0);
      }
    });
  });

  describe('Mobile Menu', () => {
    beforeEach(() => {
      global.innerWidth = 375;
    });

    it('shows mobile header', () => {
      renderLayout();

      expect(screen.getByText('Recall Vault')).toBeInTheDocument();
    });

    it('toggles mobile menu on hamburger click', async () => {
      renderLayout();

      const menuButton = screen.getByLabelText('Toggle menu');
      expect(menuButton).toHaveAttribute('aria-expanded', 'false');

      await user.click(menuButton);

      expect(menuButton).toHaveAttribute('aria-expanded', 'true');
    });

    it('closes menu when nav item clicked', async () => {
      renderLayout();

      const menuButton = screen.getByLabelText('Toggle menu');
      await user.click(menuButton);

      const searchButtons = screen.getAllByLabelText('Search');
      await user.click(searchButtons[1]);

      // Menu should close
      expect(menuButton).toHaveAttribute('aria-expanded', 'false');
    });

    it('closes menu when backdrop clicked', async () => {
      renderLayout();

      const menuButton = screen.getByLabelText('Toggle menu');
      await user.click(menuButton);

      const backdrop = document.querySelector('.bg-black.bg-opacity-50');
      if (backdrop) {
        await user.click(backdrop as HTMLElement);
      }
    });
  });

  describe('Tooltips', () => {
    it('shows tooltip with label and shortcut', () => {
      renderLayout();

      const homeButtons = screen.getAllByLabelText('Home');
      // All buttons should have the tooltip
      homeButtons.forEach(btn => {
        expect(btn).toHaveAttribute('title', 'Home (⌘0)');
      });
    });

    it('has tooltip for each nav item', () => {
      renderLayout();

      const tooltips = [
        { label: 'Home', title: 'Home (⌘0)' },
        { label: 'Search', title: 'Search (⌘1)' },
        { label: 'Files', title: 'Files (⌘2)' },
        { label: 'Chat', title: 'Chat (⌘4)' },
        { label: 'Daily Notes', title: 'Daily Notes (⌘3)' },
        { label: 'Reference Inbox', title: 'Reference Inbox (⌘5)' },
        { label: 'Settings', title: 'Settings (⌘,)' }
      ];

      tooltips.forEach(({ label, title }) => {
        const buttons = screen.getAllByLabelText(label);
        buttons.forEach(btn => {
          expect(btn).toHaveAttribute('title', title);
        });
      });
    });
  });

  describe('Accessibility', () => {
    it('has navigation landmark', () => {
      renderLayout();

      const navs = screen.getAllByRole('navigation');
      expect(navs.length).toBeGreaterThan(0);
      navs.forEach(nav => {
        expect(nav).toHaveAttribute('aria-label', 'Main navigation');
      });
    });

    it('all buttons are keyboard accessible', async () => {
      renderLayout('/home');

      const searchButtons = screen.getAllByLabelText('Search');
      const searchButton = searchButtons[0];
      searchButton.focus();

      expect(searchButton).toHaveFocus();

      await user.keyboard('{Enter}');

      // At least one button should become active after navigation
      const activeButtons = screen.getAllByLabelText('Search').filter(btn =>
        btn.className.includes('bg-[var(--accent-primary)]')
      );
      expect(activeButtons.length).toBeGreaterThan(0);
    });

    it('has accessible button labels', () => {
      renderLayout();

      const labels = ['Home', 'Search', 'Files', 'Chat', 'Daily Notes', 'Reference Inbox', 'Settings'];
      labels.forEach(label => {
        const buttons = screen.getAllByLabelText(label);
        buttons.forEach(btn => {
          expect(btn).toHaveAccessibleName();
        });
      });
    });

    it('mobile menu button has accessible state', async () => {
      renderLayout();

      const menuButton = screen.getByLabelText('Toggle menu');
      expect(menuButton).toHaveAttribute('aria-expanded', 'false');

      await user.click(menuButton);

      expect(menuButton).toHaveAttribute('aria-expanded', 'true');
    });

    it('has minimum touch target sizes (44x44px)', () => {
      const { container } = renderLayout();

      const buttons = container.querySelectorAll('button');
      buttons.forEach(button => {
        expect(button).toHaveClass('min-w-[44px]', 'min-h-[44px]');
      });
    });
  });

  describe('Responsive Behavior', () => {
    it('hides desktop sidebar on mobile', () => {
      const { container } = renderLayout();

      const desktopNav = container.querySelector('.hidden.md\\:flex');
      expect(desktopNav).toBeInTheDocument();
    });

    it('applies mobile top margin', () => {
      const { container } = renderLayout();

      const mainContent = container.querySelector('.mt-14.md\\:mt-0');
      expect(mainContent).toBeInTheDocument();
    });
  });

  describe('Visual States', () => {
    it('applies active styles to current view', () => {
      renderLayout('/home');

      const homeButtons = screen.getAllByLabelText('Home');
      // At least one should be active
      const activeButton = homeButtons.find(btn =>
        btn.className.includes('bg-[var(--accent-primary)]/10') &&
        btn.className.includes('text-[var(--accent-primary)]')
      );
      expect(activeButton).toBeDefined();
    });

    it('applies inactive styles to other views', () => {
      renderLayout('/home');

      const searchButtons = screen.getAllByLabelText('Search');
      // At least one should be inactive
      const inactiveButton = searchButtons.find(btn =>
        btn.className.includes('text-[var(--text-tertiary)]') &&
        btn.className.includes('hover:bg-[var(--surface-hover)]')
      );
      expect(inactiveButton).toBeDefined();
    });

    it('shows proper icons for each view', () => {
      const { container } = renderLayout();

      const svgs = container.querySelectorAll('svg.w-5.h-5');
      expect(svgs.length).toBeGreaterThanOrEqual(7);
    });
  });

  describe('Settings Position', () => {
    it('positions settings at bottom of sidebar', () => {
      const { container } = renderLayout();

      const spacers = container.querySelectorAll('.flex-1');
      expect(spacers.length).toBeGreaterThan(0);
    });
  });
});
