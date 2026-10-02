import { useState } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { StudioSectionErrorBoundary } from './StudioSectionErrorBoundary';

function UnavailablePanel({ shouldThrow }: { shouldThrow: boolean }) {
  if (shouldThrow) throw new Error('The plan view could not render');
  return <p>Plan view recovered</p>;
}

function StudioHarness() {
  const [shouldThrow, setShouldThrow] = useState(true);
  return (
    <>
      <p>Other app section remains available</p>
      <button type="button" onClick={() => setShouldThrow(false)}>Repair plan view</button>
      <StudioSectionErrorBoundary>
        <UnavailablePanel shouldThrow={shouldThrow} />
      </StudioSectionErrorBoundary>
    </>
  );
}

describe('StudioSectionErrorBoundary', () => {
  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('keeps the rest of the app available and retries a repaired Studio section', async () => {
    const user = userEvent.setup();
    render(<StudioHarness />);

    expect(screen.getByText("Couldn't load Studio.")).toBeVisible();
    expect(screen.getByText('Other app section remains available')).toBeVisible();
    expect(screen.queryByText('Plan view recovered')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Repair plan view' }));
    await user.click(screen.getByRole('button', { name: 'Try again' }));

    expect(await screen.findByText('Plan view recovered')).toBeVisible();
    expect(screen.getByText('Other app section remains available')).toBeVisible();
    expect(screen.queryByText("Couldn't load Studio.")).not.toBeInTheDocument();
  });
});
