import { useState } from 'react';

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { MediaLightbox } from '../MediaLightbox';

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Enlarge</button>
      <MediaLightbox
        open={open}
        onOpenChange={setOpen}
        title="Diagram"
        caption="How a request travels"
        actions={<a href="#source">Open source page</a>}
      >
        <svg data-testid="figure" />
      </MediaLightbox>
    </>
  );
}

describe('MediaLightbox', () => {
  it('opens with its figure, keeps focus inside, closes on Escape and returns focus', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole('button', { name: 'Enlarge' });
    await user.click(opener);

    const dialog = await screen.findByRole('dialog', { name: 'Diagram' });
    expect(screen.getByTestId('figure')).toBeInTheDocument();
    expect(dialog).toHaveTextContent('How a request travels');
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    for (let step = 0; step < 4; step += 1) {
      await user.tab();
      expect(dialog).toContainElement(document.activeElement as HTMLElement);
    }

    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(opener).toHaveFocus();
  });
});
