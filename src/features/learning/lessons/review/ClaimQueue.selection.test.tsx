import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ClaimQueue } from '@/features/learning/lessons/review/ClaimQueue';
import { groupBySection } from '@/features/learning/lessons/review/claimReviewPresentation';
import { unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';

const numbered = (index: number, unitIndex = index) => {
  const item = unsupportedItem(index, unitIndex);
  return { ...item, presentation: { ...item.presentation, statement: item.statement } };
};

function setup(selected: string[] = []) {
  const claims = [numbered(1, 0), numbered(2, 0), numbered(3, 1)];
  const selection = {
    isSelected: (claimId: string) => selected.includes(claimId),
    toggle: vi.fn(),
    setMany: vi.fn(),
  };
  render(<ClaimQueue groups={groupBySection(claims)} mode="section" onModeChange={vi.fn()} activeId={null}
    openedIds={new Set()} settledIds={new Set()} onSelect={vi.fn()} onAcceptSource={vi.fn()} onAddSource={vi.fn()}
    resolved={[]} teachingWaiting={0} teachingActive={false} onOpenTeaching={vi.fn()}
    removedCount={0} removedActive={false} onOpenRemoved={vi.fn()} busy={false} selection={selection} />);
  return selection;
}

describe('ClaimQueue selection', () => {
  it('gives every statement a checkbox that shows whether it is selected', () => {
    setup(['claim-2']);
    const boxes = screen.getAllByRole('checkbox', { name: 'Select this statement' });
    expect(boxes).toHaveLength(3);
    expect(boxes.map((box) => (box as HTMLInputElement).checked)).toEqual([false, true, false]);
  });

  it('toggles a statement without opening it, and says when shift was held', () => {
    const selection = setup();
    const boxes = screen.getAllByRole('checkbox', { name: 'Select this statement' });
    fireEvent.click(boxes[0]);
    expect(selection.toggle).toHaveBeenLastCalledWith('claim-1', { shiftKey: false });
    fireEvent.click(boxes[2], { shiftKey: true });
    expect(selection.toggle).toHaveBeenLastCalledWith('claim-3', { shiftKey: true });
  });

  it('selects every statement of a group from its header, and none again once all are', async () => {
    const selection = setup();
    const section = screen.getByRole('region', { name: /2 claims/ });
    await userEvent.click(within(section).getByRole('button', { name: 'Select all 2' }));
    expect(selection.setMany).toHaveBeenLastCalledWith(['claim-1', 'claim-2'], true);
  });

  it('offers Select none once a whole group is selected', async () => {
    const selection = setup(['claim-1', 'claim-2']);
    const section = screen.getByRole('region', { name: /2 claims/ });
    expect(within(section).queryByRole('button', { name: 'Select all 2' })).not.toBeInTheDocument();
    await userEvent.click(within(section).getByRole('button', { name: 'Select none' }));
    expect(selection.setMany).toHaveBeenLastCalledWith(['claim-1', 'claim-2'], false);
  });

  it('shows no checkboxes when the queue is not given a selection', () => {
    render(<ClaimQueue groups={groupBySection([numbered(1, 0)])} mode="section" onModeChange={vi.fn()} activeId={null}
      openedIds={new Set()} settledIds={new Set()} onSelect={vi.fn()} onAcceptSource={vi.fn()} onAddSource={vi.fn()}
      resolved={[]} teachingWaiting={0} teachingActive={false} onOpenTeaching={vi.fn()}
      removedCount={0} removedActive={false} onOpenRemoved={vi.fn()} busy={false} />);
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
  });
});
