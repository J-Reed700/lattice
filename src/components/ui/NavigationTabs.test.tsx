import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { NavigationTabs } from './NavigationTabs';

const options = [{ id: 'a', label: 'Alpha' }, { id: 'b', label: 'Beta' }];

function show() {
  const onValueChange = vi.fn();
  const onReselect = vi.fn();
  render(<NavigationTabs label="Sections" value="a" options={options} onValueChange={onValueChange} onReselect={onReselect} />);
  return { onValueChange, onReselect };
}

describe('NavigationTabs', () => {
  it('reports a press on the selected tab, and not a change of value', () => {
    const { onValueChange, onReselect } = show();
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Alpha' }));
    expect(onReselect).toHaveBeenCalledExactlyOnceWith('a');
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it('reports Enter and Space on the selected tab', async () => {
    const user = userEvent.setup();
    const { onReselect } = show();
    screen.getByRole('tab', { name: 'Alpha' }).focus();
    await user.keyboard('{Enter}');
    await user.keyboard(' ');
    expect(onReselect).toHaveBeenCalledTimes(2);
  });

  it('does not report choosing another tab, and still changes the value', async () => {
    const user = userEvent.setup();
    const { onValueChange, onReselect } = show();
    await user.click(screen.getByRole('tab', { name: 'Beta' }));
    expect(onValueChange).toHaveBeenCalledWith('b');
    expect(onReselect).not.toHaveBeenCalled();
  });

  it('ignores a press with the control key held', () => {
    const { onReselect } = show();
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Alpha' }), { ctrlKey: true });
    expect(onReselect).not.toHaveBeenCalled();
  });
});
