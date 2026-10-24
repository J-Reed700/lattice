import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { MessageEditor } from '@/features/chat/components/MessageEditor';

const renderEditor = (isBusy = false) => {
  const props = { onCancel: vi.fn(), onSend: vi.fn().mockResolvedValue(undefined), onSendAsBranch: vi.fn().mockResolvedValue(undefined) };
  render(<MessageEditor initialValue="When did the Ming dynasty end?" isBusy={isBusy} {...props} />);
  return props;
};

const box = () => screen.getByRole('textbox', { name: 'Edit this message' });

describe('MessageEditor', () => {
  it('opens focused on the existing text with the caret at the end', () => {
    renderEditor();
    expect(box()).toHaveFocus();
    expect((box() as HTMLTextAreaElement).selectionStart).toBe('When did the Ming dynasty end?'.length);
  });

  it('sends the trimmed edit on Enter or Send, and as a branch', () => {
    const props = renderEditor();
    fireEvent.change(box(), { target: { value: '  When did the Qing dynasty end?  ' } });
    fireEvent.keyDown(box(), { key: 'Enter', shiftKey: true });
    expect(props.onSend).not.toHaveBeenCalled();
    fireEvent.keyDown(box(), { key: 'Enter' });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    fireEvent.click(screen.getByRole('button', { name: 'Send as branch' }));
    expect(props.onSend).toHaveBeenCalledTimes(2);
    expect(props.onSend).toHaveBeenCalledWith('When did the Qing dynasty end?');
    expect(props.onSendAsBranch).toHaveBeenCalledWith('When did the Qing dynasty end?');
  });

  it('will not send a blank edit or while busy', () => {
    const props = renderEditor();
    fireEvent.change(box(), { target: { value: '   ' } });
    fireEvent.keyDown(box(), { key: 'Enter' });
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Send as branch' })).toBeDisabled();
    expect(props.onSend).not.toHaveBeenCalled();
  });

  it('cannot send while a turn runs', () => {
    renderEditor(true);
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('cancels on Escape or Cancel', () => {
    const props = renderEditor();
    fireEvent.keyDown(box(), { key: 'Escape' });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.keyDown(box(), { key: 'a' });
    expect(props.onCancel).toHaveBeenCalledTimes(2);
  });
});
