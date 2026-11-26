import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Editable } from './Editable';
import { useEditMode } from './editMode';
import { useEditorStore } from './editorStore';

const content = { kind: 'lesson.section', key: 'section-3' };

describe('Editable', () => {
  beforeEach(() => {
    useEditMode.setState({ on: false });
    useEditorStore.setState({ current: null, next: null });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders a plain contents wrapper outside edit mode and preserves child DOM identity across toggles', () => {
    const { container } = render(
      <Editable content={content} label="section 3: Title">
        <p data-testid="child-node">Quillet lesson prose</p>
      </Editable>,
    );

    const wrapper = container.firstElementChild;
    expect(wrapper?.tagName).toBe('DIV');
    expect(wrapper).toHaveClass('contents');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();

    const initialNode = screen.getByTestId('child-node');
    act(() => useEditMode.getState().set(true));
    expect(screen.getByRole('button', { name: 'Edit section 3: Title' })).toBeInTheDocument();
    expect(screen.getByTestId('child-node')).toBe(initialNode);

    act(() => useEditMode.getState().set(false));
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.getByTestId('child-node')).toBe(initialNode);
  });

  it('opens the editor from the button, or with source aim on a source-lines block, ignoring citations and selections', () => {
    useEditMode.setState({ on: true });
    const source = 'Line 0\nLine 1\nAimed line 2\nAimed line 3';

    render(
      <Editable content={content} label="section 3: Title" path="/body" source={source} sourceStart={12}>
        <article>
          <p data-source-lines="2-4">
            Body text <span data-cite="1">[1]</span>
          </p>
        </article>
      </Editable>,
    );

    const button = screen.getByRole('button', { name: 'Edit section 3: Title' });
    fireEvent.click(button);
    expect(useEditorStore.getState().current).toEqual({
      address: { content, path: null, fragment: null },
      options: { returnFocusTo: button },
    });

    useEditorStore.setState({ current: null, next: null });

    // Citation clicks inside a source-lines element do not open the editor.
    fireEvent.click(screen.getByText('[1]'));
    expect(useEditorStore.getState().current).toBeNull();

    // Non-empty text selection does not open the editor.
    vi.spyOn(window, 'getSelection').mockReturnValue({ toString: () => 'Body' } as Selection);
    fireEvent.click(screen.getByText('Body text'));
    expect(useEditorStore.getState().current).toBeNull();

    vi.restoreAllMocks();
    fireEvent.click(screen.getByText('Body text'));
    expect(useEditorStore.getState().current).toEqual({
      address: { content, path: '/body', fragment: null },
      options: {
        aim: { lines: [2, 4], source, sourceStart: 12 },
        returnFocusTo: button,
      },
    });
  });
});
