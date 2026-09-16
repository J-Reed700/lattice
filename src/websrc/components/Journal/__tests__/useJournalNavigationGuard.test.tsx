import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createMemoryRouter, Link, RouterProvider } from 'react-router';
import { expect, it, vi } from 'vitest';

import { useJournalNavigationGuard } from '../useJournalNavigationGuard';

it.each([true, false])('only leaves the editor after a successful save (%s)', async (saved) => {
  const saveNow = vi.fn().mockResolvedValue(saved);
  function Editor() {
    useJournalNavigationGuard(true, saveNow);
    return <Link to="/away">Leave editor</Link>;
  }
  const router = createMemoryRouter([
    { path: '/', element: <Editor /> },
    { path: '/away', element: <p>Destination</p> },
  ]);
  render(<RouterProvider router={router} />);
  fireEvent.click(screen.getByRole('link'));
  await waitFor(() => expect(saveNow).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(router.state.location.pathname).toBe(saved ? '/away' : '/'));
  if (!saved) expect(screen.getByRole('link')).toBeVisible();
});
