import { act, fireEvent, screen } from '@testing-library/react';

import type { UserEvent } from '@testing-library/user-event';

/** Polyfill pointer-capture and scrollIntoView APIs required by Radix Select in jsdom. */
export function polyfillRadixSelect() {
  if (!Element.prototype.hasPointerCapture) {
    Element.prototype.hasPointerCapture = () => false;
  }
  if (!Element.prototype.setPointerCapture) {
    Element.prototype.setPointerCapture = () => {};
  }
  if (!Element.prototype.releasePointerCapture) {
    Element.prototype.releasePointerCapture = () => {};
  }
  if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = () => {};
  }
}

/** Open a Radix Select trigger and pick an option by accessible name. */
export async function chooseRadixOption(
  trigger: HTMLElement,
  optionName: string | RegExp,
  _user?: UserEvent,
) {
  polyfillRadixSelect();
  act(() => {
    trigger.focus();
  });
  fireEvent.keyDown(trigger, { key: 'ArrowDown' });
  const option = await screen.findByRole('option', { name: optionName });
  act(() => {
    option.focus();
  });
  fireEvent.keyDown(option, { key: 'Enter' });
}
