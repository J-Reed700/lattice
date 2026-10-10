import { useState } from 'react';

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';

function Harness({ hideClose = false, unstyled = false }: { hideClose?: boolean; unstyled?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Open settings</button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent hideClose={hideClose} unstyled={unstyled} aria-describedby={undefined} className="custom-surface">
          <DialogTitle asChild><h3 className="own-title">Reset everything?</h3></DialogTitle>
          <button type="button" onClick={() => setOpen(false)}>Keep</button>
        </DialogContent>
      </Dialog>
    </>
  );
}

describe('the dialog primitive', () => {
  it('returns focus to what opened it when no trigger is wired', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole('button', { name: 'Open settings' });

    await user.click(opener);
    expect(screen.getByRole('dialog', { name: 'Reset everything?' })).toBeVisible();
    await user.keyboard('{Escape}');

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it('draws its close button unless the dialog has its own way out', async () => {
    const user = userEvent.setup();
    const { unmount } = render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Open settings' }));
    expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument();
    unmount();

    render(<Harness hideClose />);
    await user.click(screen.getByRole('button', { name: 'Open settings' }));
    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
  });

  it('keeps a title passed as its own element styled as it is', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Open settings' }));

    expect(screen.getByRole('heading', { name: 'Reset everything?' })).toHaveAttribute('class', 'own-title');
  });

  it('applies only the caller\'s classes to an unstyled surface', async () => {
    const user = userEvent.setup();
    render(<Harness unstyled />);
    await user.click(screen.getByRole('button', { name: 'Open settings' }));

    expect(screen.getByRole('dialog')).toHaveAttribute('class', 'custom-surface');
  });

  it('merges onto the caller\'s own element with asChild', async () => {
    render(
      <Dialog open>
        <DialogContent asChild unstyled hideClose aria-describedby={undefined}>
          <aside className="own-panel">
            <DialogTitle>Spaces</DialogTitle>
          </aside>
        </DialogContent>
      </Dialog>,
    );

    const panel = screen.getByRole('dialog', { name: 'Spaces' });
    expect(panel.tagName).toBe('ASIDE');
    expect(panel).toHaveClass('own-panel');
  });
});
