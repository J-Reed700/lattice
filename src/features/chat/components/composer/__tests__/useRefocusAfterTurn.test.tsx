import { useRef } from 'react';

import { render } from '@testing-library/react';
import { expect, it } from 'vitest';

import { useRefocusAfterTurn } from '@/features/chat/components/composer/useRefocusAfterTurn';

function Composer({ isSending }: { isSending: boolean }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useRefocusAfterTurn(isSending, ref);
  return (
    <>
      <textarea ref={ref} aria-label="composer" />
      <input aria-label="elsewhere" />
    </>
  );
}

it('puts the caret back in the composer when the turn ends and focus had fallen to the page', () => {
  const { rerender, getByLabelText } = render(<Composer isSending />);
  (document.activeElement as HTMLElement | null)?.blur();
  expect(document.activeElement).toBe(document.body);

  rerender(<Composer isSending={false} />);

  expect(document.activeElement).toBe(getByLabelText('composer'));
});

it('leaves focus alone when the reader moved to another field during the turn', () => {
  const { rerender, getByLabelText } = render(<Composer isSending />);
  getByLabelText('elsewhere').focus();

  rerender(<Composer isSending={false} />);

  expect(document.activeElement).toBe(getByLabelText('elsewhere'));
});

it('does not grab focus on first render', () => {
  render(<Composer isSending={false} />);
  expect(document.activeElement).toBe(document.body);
});
