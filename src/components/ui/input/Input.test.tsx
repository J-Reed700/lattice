import { createRef } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import Input from './Input';

describe('Input', () => {
  it('labels the field and describes it with helper text', () => {
    const ref = createRef<HTMLInputElement>();
    render(<Input ref={ref} id="title" label="Recipe title" helperText="Shown on the card" />);
    const field = screen.getByLabelText('Recipe title');
    expect(ref.current).toBe(field);
    expect(field).toHaveAttribute('aria-describedby', 'title-helper');
    expect(field).toHaveAttribute('aria-invalid', 'false');
    expect(screen.getByText('Shown on the card')).toHaveAttribute('id', 'title-helper');
  });

  it('shows an error in place of the helper text', () => {
    render(<Input id="title" label="Recipe title" helperText="Shown on the card" error="A title is required" />);
    const field = screen.getByLabelText('Recipe title');
    expect(field).toHaveAttribute('aria-invalid', 'true');
    expect(field).toHaveAttribute('aria-describedby', 'title-error');
    expect(screen.getByRole('alert')).toHaveTextContent('A title is required');
    expect(screen.queryByText('Shown on the card')).not.toBeInTheDocument();
  });

  it('gives an unlabelled field its own id', () => {
    const { rerender } = render(<Input label="Servings" />);
    const id = screen.getByLabelText('Servings').id;
    expect(id).toMatch(/^input-/);
    rerender(<Input label="Servings" helperText="Number of guests" />);
    expect(screen.getByLabelText('Servings').id).toBe(id);
  });

  it('counts an uncontrolled field and a controlled numeric zero', () => {
    const { rerender } = render(<Input label="Name" defaultValue="salt" maxLength={10} showCharCount />);
    expect(screen.getByText('4 / 10')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'pepper' } });
    expect(screen.getByText('6 / 10')).toBeInTheDocument();
    rerender(<Input key="count" label="Count" value={0} onChange={() => {}} maxLength={10} showCharCount />);
    expect(screen.getByText('1 / 10')).toBeInTheDocument();
  });

  it('validates on blur when asked, clearing the error once fixed', () => {
    const onBlur = vi.fn();
    const validate = (value: string) => (value.length < 3 ? 'Too short' : undefined);
    render(<Input label="Name" validateOnBlur validate={validate} onBlur={onBlur} />);
    const field = screen.getByLabelText('Name');
    fireEvent.blur(field, { target: { value: 'ab' } });
    expect(screen.getByRole('alert')).toHaveTextContent('Too short');
    fireEvent.blur(field, { target: { value: 'abcd' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(onBlur).toHaveBeenCalledTimes(2);
  });

  it('does not validate on blur unless both the flag and a validator are given', () => {
    const validate = vi.fn(() => 'never');
    render(<Input label="Name" validate={validate} />);
    fireEvent.blur(screen.getByLabelText('Name'));
    expect(validate).not.toHaveBeenCalled();
  });

  it('disables the field and shows a spinner while loading', () => {
    const { container } = render(<Input label="Name" isLoading rightIcon={<span>icon</span>} />);
    expect(screen.getByLabelText('Name')).toBeDisabled();
    expect(container.querySelector('.animate-spin')).not.toBeNull();
    expect(screen.queryByText('icon')).not.toBeInTheDocument();
  });

  it('shows the icons it is given', () => {
    render(<Input label="Name" leftIcon={<span>left</span>} rightIcon={<span>right</span>} />);
    expect(screen.getByText('left')).toBeInTheDocument();
    expect(screen.getByText('right')).toBeInTheDocument();
    expect(screen.getByLabelText('Name').className).toContain('pl-10');
  });

  it('offers a clear button only when there is text, and clears a validation error with it', () => {
    const onClear = vi.fn();
    const { rerender } = render(<Input label="Name" value="" onChange={() => {}} showClearButton onClear={onClear} />);
    expect(screen.queryByRole('button', { name: 'Clear input' })).not.toBeInTheDocument();
    rerender(<Input label="Name" value="x" onChange={() => {}} showClearButton onClear={onClear} validateOnBlur validate={() => 'Too short'} />);
    fireEvent.blur(screen.getByLabelText('Name'));
    expect(screen.getByRole('alert')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear input' }));
    expect(onClear).toHaveBeenCalled();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear input' })).not.toBeInTheDocument();
  });

  it('counts characters against the limit and warns near it', () => {
    const { rerender } = render(<Input label="Name" value="salt" onChange={() => {}} maxLength={10} showCharCount />);
    expect(screen.getByText('4 / 10')).toHaveClass('text-[hsl(var(--text-secondary))]');
    rerender(<Input label="Name" value="saltpepper" onChange={() => {}} maxLength={11} showCharCount />);
    expect(screen.getByText('10 / 11')).toHaveClass('text-[hsl(var(--warning-fg))]');
    rerender(<Input label="Name" value="salt" onChange={() => {}} showCharCount />);
    expect(screen.queryByText(/\/ 10/)).not.toBeInTheDocument();
  });

  it('marks a full field with the danger colour', () => {
    render(<Input label="Name" value="saltpepper" onChange={() => {}} maxLength={10} showCharCount />);
    expect(screen.getByText('10 / 10')).toHaveClass('text-[hsl(var(--danger-fg))]');
  });
});
