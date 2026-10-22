import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ClaimQueue } from '@/features/learning/lessons/review/ClaimQueue';
import { groupBySection } from '@/features/learning/lessons/review/claimReviewPresentation';
import { claimItem, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';

// The queue shows each claim's presented statement, so number that too.
const numbered = (index: number, unitIndex = index) => {
  const item = unsupportedItem(index, unitIndex);
  return { ...item, presentation: { ...item.presentation, statement: item.statement } };
};

const proposal = (index: number) => claimItem({ ...numbered(index), verdictKind: { kind: 'general_knowledge_proposed' } });

function setup(overrides: Partial<Parameters<typeof ClaimQueue>[0]> = {}) {
  const claims = overrides.groups?.flatMap((group) => group.claims) ?? [numbered(1, 0), numbered(2, 0), numbered(3, 1)];
  const props = {
    groups: groupBySection(claims), mode: 'section' as const, onModeChange: vi.fn(), activeId: null as string | null,
    openedIds: new Set<string>(), settledIds: new Set<string>(), onSelect: vi.fn(), onAcceptSource: vi.fn(), onAddSource: vi.fn(),
    onAcceptProposals: vi.fn(), resolved: [], teachingWaiting: 0, teachingActive: false, onOpenTeaching: vi.fn(), busy: false,
    ...overrides,
  };
  render(<ClaimQueue {...props} />);
  return props;
}

describe('ClaimQueue', () => {
  it('says everything is resolved when no claim waits', () => {
    setup({ groups: [] });
    expect(screen.getByText('Everything is resolved.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Add a source for these/ })).not.toBeInTheDocument();
  });

  it('changes the grouping mode from the select', async () => {
    const props = setup();
    await userEvent.selectOptions(screen.getByLabelText('Group by'), 'fix');
    expect(props.onModeChange).toHaveBeenCalledWith('fix');
  });

  it('marks rows opened, unopened and the one in focus, and selects on click', async () => {
    const props = setup({ activeId: 'claim-2', openedIds: new Set(['claim-1', 'claim-2']) });
    const rows = screen.getAllByRole('button', { name: /Statement number/ });
    expect(rows[0]).toHaveTextContent('●');
    expect(rows[2]).toHaveTextContent('○');
    expect(rows[2]).toHaveTextContent('not opened yet');
    expect(rows[1]).toHaveAttribute('aria-current', 'true');
    expect(rows[1]).toHaveAttribute('tabindex', '0');
    expect(rows[0]).toHaveAttribute('tabindex', '-1');
    await userEvent.click(rows[2]);
    expect(props.onSelect).toHaveBeenCalledWith('claim-3');
  });

  it('gives the first claim the tab stop when none is in focus', () => {
    setup();
    expect(screen.getByRole('button', { name: /Statement number 1\./ })).toHaveAttribute('tabindex', '0');
  });

  it('shows a settled claim with its resolved chip', () => {
    const settled = claimItem({ ...numbered(1, 0), state: { state: 'confirmed', at: 5 } });
    setup({ groups: groupBySection([settled]), settledIds: new Set(['claim-1']) });
    expect(screen.getByRole('button', { name: /Statement number 1\./ })).toHaveTextContent('Confirmed by you');
  });

  it('collapses and reopens a group from its header', async () => {
    setup();
    const section = screen.getByRole('region', { name: /2 claims/ });
    const header = within(section).getAllByRole('button')[0];
    expect(header).toHaveAttribute('aria-expanded', 'true');
    await userEvent.click(header);
    expect(header).toHaveAttribute('aria-expanded', 'false');
    expect(within(section).queryByRole('button', { name: /Statement number 1\./ })).not.toBeInTheDocument();
    await userEvent.click(header);
    expect(within(section).getByRole('button', { name: /Statement number 1\./ })).toBeInTheDocument();
  });

  it('reopens a collapsed group when its claim comes into focus', async () => {
    const claims = [numbered(1, 0), numbered(2, 0), numbered(3, 1)];
    const groups = groupBySection(claims);
    const base = {
      groups, mode: 'section' as const, onModeChange: vi.fn(), openedIds: new Set<string>(), settledIds: new Set<string>(), onSelect: vi.fn(),
      onAcceptSource: vi.fn(), onAddSource: vi.fn(), onAcceptProposals: vi.fn(), resolved: [], teachingWaiting: 0, teachingActive: false, onOpenTeaching: vi.fn(), busy: false,
    };
    const { rerender } = render(<ClaimQueue {...base} activeId="claim-3" />);
    const first = screen.getAllByRole('region')[0];
    await userEvent.click(within(first).getAllByRole('button')[0]);
    expect(within(first).queryByRole('button', { name: /Statement number 1\./ })).not.toBeInTheDocument();
    rerender(<ClaimQueue {...base} activeId="claim-1" />);
    expect(await screen.findByRole('button', { name: /Statement number 1\./ })).toBeInTheDocument();
  });

  it('offers group and queue bulk actions, and disables them while busy', async () => {
    const props = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Add a source for these 3' }));
    expect(props.onAddSource).toHaveBeenLastCalledWith(expect.objectContaining({ key: 'queue', source: null, claims: expect.arrayContaining([expect.objectContaining({ claimId: 'claim-3' })]) }));
    const section = screen.getByRole('region', { name: /2 claims/ });
    await userEvent.click(within(section).getByRole('button', { name: 'Add a source for these 2' }));
    expect(props.onAddSource).toHaveBeenLastCalledWith(expect.objectContaining({ key: '0' }));
    // A group of one gets no bulk row; a group without a source offers no accept.
    expect(within(screen.getAllByRole('region')[1]).queryByRole('button', { name: /Add a source for these/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Accept this source for all/ })).not.toBeInTheDocument();
  });

  it('accepts a source for a whole source group', async () => {
    const source = { sourceId: 's', sourceVersionId: 'v', title: 'Guide', url: null, addedBy: 'research' as const };
    const claims = [numbered(1, 0), numbered(2, 1)];
    const groups = [{ key: 'source-s', label: 'Guide', detail: 'example.org · online book', claims, source }];
    const props = setup({ groups, busy: false });
    expect(screen.getByText('example.org · online book · 2 claims')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Accept this source for all 2' }));
    expect(props.onAcceptSource).toHaveBeenCalledWith(groups[0]);
  });

  it('disables bulk actions while an action is saving', () => {
    setup({ groups: groupBySection([proposal(1), proposal(2)]), busy: true });
    expect(screen.getByRole('button', { name: 'Add a source for these 2' })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Accept all 2 common-knowledge proposals/ })).toBeDisabled();
  });

  it('accepts every unsettled common-knowledge proposal at once', async () => {
    const props = setup({ groups: groupBySection([proposal(1), proposal(2), numbered(3, 1)]), settledIds: new Set(['claim-2']) });
    await userEvent.click(screen.getByRole('button', { name: 'Accept all 1 common-knowledge proposal' }));
    expect(props.onAcceptProposals).toHaveBeenCalledWith([expect.objectContaining({ claimId: 'claim-1' })]);
  });

  it('moves between rows with the arrow keys, Home and End', () => {
    setup();
    const rows = screen.getAllByRole('button', { name: /Statement number/ });
    rows[0].focus();
    fireEvent.keyDown(rows[0], { key: 'ArrowDown' });
    expect(rows[1]).toHaveFocus();
    fireEvent.keyDown(rows[1], { key: 'End' });
    expect(rows[2]).toHaveFocus();
    fireEvent.keyDown(rows[2], { key: 'ArrowDown' });
    expect(rows[2]).toHaveFocus();
    fireEvent.keyDown(rows[2], { key: 'ArrowUp' });
    expect(rows[1]).toHaveFocus();
    fireEvent.keyDown(rows[1], { key: 'Home' });
    expect(rows[0]).toHaveFocus();
    fireEvent.keyDown(rows[0], { key: 'ArrowUp' });
    expect(rows[0]).toHaveFocus();
  });

  it('leaves other keys and non-row targets alone', () => {
    setup();
    const rows = screen.getAllByRole('button', { name: /Statement number/ });
    rows[0].focus();
    expect(fireEvent.keyDown(rows[0], { key: 'a' })).toBe(true);
    const header = screen.getAllByRole('region')[0].querySelector('button') as HTMLElement;
    expect(fireEvent.keyDown(header, { key: 'ArrowDown' })).toBe(true);
  });

  it('opens the teaching notes row with the count of notes waiting', async () => {
    const props = setup({ teachingWaiting: 2, teachingActive: true });
    const row = screen.getByRole('button', { name: /Teaching notes/ });
    expect(row).toHaveTextContent('2 teaching notes for you');
    expect(row).toHaveAttribute('aria-current', 'true');
    await userEvent.click(row);
    expect(props.onOpenTeaching).toHaveBeenCalled();
  });

  it('hides the teaching notes row when none wait', () => {
    setup();
    expect(screen.queryByRole('region', { name: 'Teaching notes' })).not.toBeInTheDocument();
  });

  it('shows resolved claims behind a toggle and selects one', async () => {
    const done = claimItem({ ...numbered(9, 2), state: { state: 'general_knowledge', at: 1, by: 'user' } });
    const props = setup({ resolved: [done], activeId: 'claim-9' });
    expect(screen.queryByRole('list', { name: 'Resolved claims' })).not.toBeInTheDocument();
    const toggle = screen.getByRole('button', { name: 'Show resolved (1)' });
    await userEvent.click(toggle);
    expect(toggle).toHaveTextContent('Hide resolved');
    const row = within(screen.getByRole('list', { name: 'Resolved claims' })).getByRole('button');
    expect(row).toHaveTextContent('Common knowledge');
    expect(row).toHaveAttribute('aria-current', 'true');
    await userEvent.click(row);
    expect(props.onSelect).toHaveBeenCalledWith('claim-9');
    await userEvent.click(toggle);
    expect(screen.queryByRole('list', { name: 'Resolved claims' })).not.toBeInTheDocument();
  });
});
