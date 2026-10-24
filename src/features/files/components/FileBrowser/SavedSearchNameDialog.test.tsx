import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { SavedSearchNameDialog } from './SavedSearchNameDialog';

const renderDialog = (mode: 'create' | 'rename' | null, open = true) => {
  const props = { onChangeValue: vi.fn(), onClose: vi.fn(), onSubmit: vi.fn() };
  render(<SavedSearchNameDialog open={open} mode={mode} value="Spice routes" {...props} />);
  return props;
};

describe('SavedSearchNameDialog', () => {
  it('saves a new search', () => {
    const props = renderDialog('create');
    expect(screen.getByRole('heading', { name: 'Save Search' })).toBeInTheDocument();
    const input = screen.getByPlaceholderText('Search name');
    fireEvent.change(input, { target: { value: 'Silk roads' } });
    expect(props.onChangeValue).toHaveBeenCalledWith('Silk roads');
    fireEvent.keyDown(input, { key: 'a' });
    expect(props.onSubmit).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter' });
    fireEvent.click(screen.getByRole('button', { name: 'Save Search' }));
    expect(props.onSubmit).toHaveBeenCalledTimes(2);
  });

  it('renames a saved search and closes', () => {
    const props = renderDialog('rename');
    expect(screen.getByRole('heading', { name: 'Rename Saved Search' })).toBeInTheDocument();
    expect(screen.getByText('Update the name of this saved search.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Rename' }));
    expect(props.onSubmit).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalledTimes(2);
  });

  it('renders nothing while closed', () => {
    renderDialog(null, false);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
