import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { JournalContextRail } from '@/features/journal/components/JournalContextRail';

describe('JournalContextRail sources tab', () => {
  it('shows the Sources tab and activates its citation content on click', () => {
    render(
      <TooltipProvider>
        <JournalContextRail
          selectedEntryId="entry-1"
          highlightCount={2}
          sourceCount={1}
          conversation={<div>Conversation content</div>}
          highlights={<div>Highlight content</div>}
          sources={<div>Saved citation content</div>}
          footer={<div>Synthesize</div>}
          onClose={vi.fn()}
        />
      </TooltipProvider>,
    );

    const sourcesTab = screen.getByRole('tab', { name: /Sources/ });
    expect(sourcesTab.getAttribute('aria-selected')).toBe('false');
    fireEvent.click(sourcesTab);

    expect(sourcesTab.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('tabpanel', { name: 'Sources and citations' })).toHaveTextContent('Saved citation content');
  });
});
