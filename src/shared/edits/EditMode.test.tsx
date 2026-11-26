import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { useEditMode } from './editMode';
import { EditModeBar } from './EditModeBar';
import { EditModeToggle } from './EditModeToggle';

describe('EditModeToggle and EditModeBar', () => {
  beforeEach(() => {
    useEditMode.setState({ on: false });
  });

  it('toggles mode with aria-pressed, announces status sentences, and closes on Done or unguarded Escape', () => {
    render(
      <>
        <EditModeToggle />
        <EditModeBar />
      </>,
    );

    const toggle = screen.getByRole('button', { name: 'Edit mode' });
    const status = screen.getByRole('status');
    expect(toggle).toHaveAttribute('aria-pressed', 'false');
    expect(status).toHaveTextContent('');
    expect(screen.queryByRole('region', { name: 'Edit mode' })).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-pressed', 'true');
    expect(status).toHaveTextContent('Edit mode on. Each piece you can correct has an Edit button.');
    expect(screen.getByRole('region', { name: 'Edit mode' })).toHaveTextContent(
      'Edit mode: choose a piece to correct it.',
    );

    // Escape does not turn off edit mode while a dialog is present in the document.
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.appendChild(dialog);
    try {
      fireEvent.keyDown(window, { key: 'Escape' });
      expect(useEditMode.getState().on).toBe(true);
    } finally {
      dialog.remove();
    }

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(useEditMode.getState().on).toBe(false);
    expect(status).toHaveTextContent('Edit mode off.');

    fireEvent.click(toggle);
    expect(useEditMode.getState().on).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(useEditMode.getState().on).toBe(false);
    expect(status).toHaveTextContent('Edit mode off.');
  });
});
