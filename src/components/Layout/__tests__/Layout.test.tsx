import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '../../ui/tooltip';
import { Layout } from '../Layout';

vi.mock('../../Downloads/DownloadsDrawer', () => ({ DownloadsDrawer: () => null }));
vi.mock('../../Downloads/DrawerTrigger', () => ({ DrawerTrigger: () => null }));
vi.mock('../../Downloads/HeaderDownloadsIndicator', () => ({
  HeaderDownloadsIndicator: () => null,
}));
vi.mock('../../IndexingStatus/IndexingStatusRail', () => ({ IndexingStatusRail: () => null }));

// The rail renders `null` under the mock above (and on a real idle vault), so
// this list is still the complete set of buttons Layout puts in the nav.
const NAV_LABELS = ['Home', 'Search', 'Library', 'Journal', 'Chat', 'References', 'Study', 'Import', 'Settings'];

describe('Layout', () => {
  let user: ReturnType<typeof userEvent.setup>;

  const renderLayout = (initialPath = '/home') =>
    render(
      <TooltipProvider>
        <MemoryRouter initialEntries={[initialPath]}>
          <Routes>
            <Route element={<Layout />}>
              <Route path="*" element={<div data-testid="outlet" />} />
            </Route>
          </Routes>
        </MemoryRouter>
      </TooltipProvider>,
    );

  beforeEach(() => {
    user = userEvent.setup();
    vi.clearAllMocks();
  });

  it('renders every navigation item once, in rail order', () => {
    renderLayout();
    const nav = screen.getByRole('navigation', { name: 'Main navigation' });
    const buttons = Array.from(nav.querySelectorAll('button')).map((b) => b.getAttribute('aria-label'));
    expect(buttons).toEqual(NAV_LABELS);
  });

  it('marks the current route with aria-current', () => {
    renderLayout('/search');
    expect(screen.getByLabelText('Search')).toHaveAttribute('aria-current', 'page');
    expect(screen.getByLabelText('Home')).not.toHaveAttribute('aria-current');
  });

  it('maps /daily to the Journal item', () => {
    renderLayout('/daily');
    expect(screen.getByLabelText('Journal')).toHaveAttribute('aria-current', 'page');
  });

  it('navigates when an item is clicked', async () => {
    renderLayout('/home');
    await user.click(screen.getByLabelText('Library'));
    expect(screen.getByLabelText('Library')).toHaveAttribute('aria-current', 'page');
    expect(screen.getByLabelText('Home')).not.toHaveAttribute('aria-current');
  });

  it('renders the routed content', () => {
    renderLayout('/chat');
    expect(screen.getByTestId('outlet')).toBeInTheDocument();
  });
});
