import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ModeChips } from '../ModeChips';

import type { ModeChipsProps } from '../ModeChips';

const renderChips = (overrides: Partial<ModeChipsProps> = {}) => {
  const onRemove = vi.fn();
  const onRemoveTool = vi.fn();
  const result = render(
    <ModeChips
      turnMode="auto"
      knowledgeBase={false}
      webSearch={false}
      wikipedia={false}
      deepResearch={false}
      customTools={[]}
      onRemove={onRemove}
      onRemoveTool={onRemoveTool}
      {...overrides}
    />
  );
  return { onRemove, onRemoveTool, ...result };
};

describe('the composer mode chips', () => {
  it('shows nothing when nothing is switched on', () => {
    const { container } = renderChips();
    expect(container).toBeEmptyDOMElement();
  });

  // A turn that will take two hours should not be a hidden checkbox.
  it('names every mode that is not the default', () => {
    renderChips({
      turnMode: 'query',
      deepResearch: true,
      webSearch: true,
      wikipedia: true,
      knowledgeBase: true,
      customTools: ['read_inbox'],
    });

    expect(screen.getByText('Query')).toBeInTheDocument();
    expect(screen.getByText('Deep research')).toBeInTheDocument();
    expect(screen.getByText('Web')).toBeInTheDocument();
    expect(screen.getByText('Wikipedia')).toBeInTheDocument();
    expect(screen.getByText('Your documents')).toBeInTheDocument();
    expect(screen.getByText('Read Inbox')).toBeInTheDocument();
  });

  it('calls Auto the default, so it earns no chip', () => {
    renderChips({ turnMode: 'auto' });
    expect(screen.queryByText('Auto')).not.toBeInTheDocument();
  });

  it('takes a mode off where it is shown', async () => {
    const { onRemove } = renderChips({ deepResearch: true });
    await userEvent.click(screen.getByRole('button', { name: /Turn off deep research/ }));
    expect(onRemove).toHaveBeenCalledWith('deep');
  });

  it('puts the turn mode back to Auto rather than removing it', async () => {
    const { onRemove } = renderChips({ turnMode: 'followup' });
    await userEvent.click(screen.getByRole('button', { name: /Back to Auto/ }));
    expect(onRemove).toHaveBeenCalledWith('turn');
  });

  it('takes a custom tool off by name', async () => {
    const { onRemoveTool } = renderChips({ customTools: ['read_inbox'] });
    await userEvent.click(screen.getByRole('button', { name: /Turn off Read Inbox/ }));
    expect(onRemoveTool).toHaveBeenCalledWith('read_inbox');
  });
});
