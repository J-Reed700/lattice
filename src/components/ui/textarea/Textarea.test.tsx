import { createRef } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Textarea } from './Textarea';

describe('Textarea', () => {
  it('labels the field, forwards ref, and connects helper text', () => {
    const ref = createRef<HTMLTextAreaElement>();
    render(<Textarea ref={ref} id="notes" label="Notes" helperText="Markdown supported" />);
    const field = screen.getByLabelText('Notes');
    expect(ref.current).toBe(field);
    expect(field).toHaveAttribute('aria-describedby', 'notes-helper');
    expect(field).toHaveAttribute('aria-invalid', 'false');
    expect(screen.getByText('Markdown supported')).toHaveAttribute('id', 'notes-helper');
  });

  it('generates a stable deterministic id via useId across rerenders', () => {
    const { rerender } = render(<Textarea label="Summary" />);
    const id = screen.getByLabelText('Summary').id;
    expect(id).toMatch(/^textarea-/);
    rerender(<Textarea label="Summary" helperText="Keep it concise" />);
    expect(screen.getByLabelText('Summary').id).toBe(id);
  });

  it('renders error state with alert role and hides helper text', () => {
    render(<Textarea id="bio" label="Bio" helperText="Public profile" error="Bio is required" />);
    const field = screen.getByLabelText('Bio');
    expect(field).toHaveAttribute('aria-invalid', 'true');
    expect(field).toHaveAttribute('aria-describedby', 'bio-error');
    expect(screen.getByRole('alert')).toHaveTextContent('Bio is required');
    expect(screen.queryByText('Public profile')).not.toBeInTheDocument();
  });

  it('tracks character count for uncontrolled and controlled inputs', () => {
    const { rerender } = render(<Textarea label="Prompt" defaultValue="hello" maxLength={20} showCharCount />);
    expect(screen.getByText('5 / 20')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Prompt'), { target: { value: 'hello world' } });
    expect(screen.getByText('11 / 20')).toBeInTheDocument();

    rerender(<Textarea label="Prompt" value="1234567890123456789" onChange={() => {}} maxLength={20} showCharCount />);
    expect(screen.getByText('19 / 20')).toHaveClass('text-warning-fg');

    rerender(<Textarea label="Prompt" value="12345678901234567890" onChange={() => {}} maxLength={20} showCharCount />);
    expect(screen.getByText('20 / 20')).toHaveClass('text-danger-fg');
  });

  it('validates on blur when validateOnBlur and validate are provided', () => {
    const onBlur = vi.fn();
    render(
      <Textarea
        label="Instructions"
        validateOnBlur
        validate={(value) => (value.trim().length < 5 ? 'Too short' : undefined)}
        onBlur={onBlur}
      />,
    );
    const field = screen.getByLabelText('Instructions');
    fireEvent.blur(field, { target: { value: 'hi' } });
    expect(screen.getByRole('alert')).toHaveTextContent('Too short');
    fireEvent.blur(field, { target: { value: 'detailed prompt' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(onBlur).toHaveBeenCalledTimes(2);
  });

  it('applies resize utility classes', () => {
    render(<Textarea label="Fixed" resize="none" />);
    expect(screen.getByLabelText('Fixed').className).toContain('resize-none');
  });
});
