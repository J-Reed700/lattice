import { useState } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { NavigationTabs } from '@/components/ui/NavigationTabs';
import { PairingBoard } from '@/components/ui/PairingBoard';
import { ResizableColumns } from '@/components/ui/ResizableColumns';
import { SearchableSelect } from '@/components/ui/SearchableSelect';
import '@/tests/interactionDom';

const options = [
  { value: 'a', label: 'Foundations', description: '2 of 3 lessons complete', keywords: ['observation'] },
  { value: 'b', label: 'Putting it together', description: 'Ready to study' },
];

function Picker({ onChange = vi.fn() }: { onChange?: (value: string) => void }) {
  const [value, setValue] = useState('a');
  return <SearchableSelect label="Module" value={value} options={options} onValueChange={(next) => { onChange(next); setValue(next); }} />;
}

describe('shared study interactions', () => {
  it('searches descriptive keywords, confirms by keyboard, and returns focus on Escape', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    const picker = screen.getByRole('combobox', { name: 'Module' });
    await user.click(picker);
    await user.type(screen.getByRole('combobox', { name: 'Search module' }), 'observation');
    expect(screen.getByRole('option', { name: /Foundations/ })).toBeVisible();
    expect(screen.queryByRole('option', { name: /Putting it/ })).not.toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(onChange).not.toHaveBeenCalled();
    expect(picker).toHaveFocus();
    await user.click(picker);
    await user.type(screen.getByRole('combobox', { name: 'Search module' }), 'together');
    await user.keyboard('{Enter}');
    expect(onChange).toHaveBeenCalledWith('b');
    expect(picker).toHaveTextContent('Putting it together');
    expect(picker).toHaveAttribute('aria-expanded', 'false');
  });

  it('shows an empty search state and does not change the selected value', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    await user.click(screen.getByRole('combobox', { name: 'Module' }));
    await user.type(screen.getByRole('combobox', { name: 'Search module' }), 'missingxyz');
    expect(screen.getByText('No results found.')).toBeVisible();
    await user.keyboard('{Enter}{Escape}');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('moves tab focus without changing the panel until confirmation', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<NavigationTabs label="Workspace" value="a" options={[{ id: 'a', label: 'Read' }, { id: 'b', label: 'Practice' }]} onValueChange={onChange} />);
    screen.getByRole('tab', { name: 'Read' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Practice' })).toHaveFocus();
    expect(onChange).not.toHaveBeenCalled();
    await user.keyboard('{Enter}');
    expect(onChange).toHaveBeenCalledWith('b');
  });

  it('resizes by keyboard, clamps at the boundaries, persists, and resets', async () => {
    localStorage.removeItem('test-pane-width');
    const user = userEvent.setup();
    const view = render(<ResizableColumns label="Notes width" storageKey="test-pane-width" sidebar={<p>Notes</p>}><p>Lesson</p></ResizableColumns>);
    const separator = screen.getByRole('separator', { name: 'Notes width' });
    separator.focus();
    await user.keyboard('{ArrowLeft}');
    expect(separator).toHaveAttribute('aria-valuenow', '261');
    expect(localStorage.getItem('test-pane-width')).toBe('261');
    await user.keyboard('{End}{ArrowLeft}');
    expect(separator).toHaveAttribute('aria-valuenow', '420');
    view.unmount();
    render(<ResizableColumns label="Notes width" storageKey="test-pane-width" sidebar={<p>Notes</p>}><p>Lesson</p></ResizableColumns>);
    expect(screen.getByRole('separator')).toHaveAttribute('aria-valuenow', '420');
    fireEvent.doubleClick(screen.getByRole('separator'));
    expect(screen.getByRole('separator')).toHaveAttribute('aria-valuenow', '245');
    localStorage.removeItem('test-pane-width');
  });

  it('pairs and clears answers with the keyboard, preserving the existing response values', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    function Board() {
      const [matches, setMatches] = useState<(number | null)[]>([null, null]);
      return <><h2 id="question">Match the terms</h2><PairingBoard leftItems={['Competitive', 'Uncompetitive']} rightItems={['Same Vmax', 'Lower Vmax and Km']} matches={matches} onChange={(next) => { onChange(next); setMatches(next); }} labelledBy="question" /></>;
    }
    render(<Board />);
    screen.getByRole('button', { name: 'Uncompetitive' }).focus();
    await user.keyboard('{Enter}');
    expect(screen.getByRole('button', { name: 'Same Vmax' })).toHaveFocus();
    await user.tab();
    await user.keyboard(' ');
    expect(onChange).toHaveBeenLastCalledWith([null, 1]);
    expect(screen.getByRole('status')).toHaveTextContent('Uncompetitive matched to Lower Vmax and Km.');
    await user.click(screen.getByRole('button', { name: 'Clear match for Uncompetitive' }));
    expect(onChange).toHaveBeenLastCalledWith([null, null]);
  });
});
