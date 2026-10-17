import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { focusReturnTarget } from '../focusReturn';

// jsdom, like macOS WebKit, does not focus a button on mousedown.
function press(element: Element, prevent = false) {
  const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
  if (prevent) element.addEventListener('mousedown', (e) => e.preventDefault(), { once: true });
  element.dispatchEvent(event);
}

describe('focusReturnTarget', () => {
  let input: HTMLInputElement;
  let button: HTMLButtonElement;

  beforeEach(() => {
    document.body.innerHTML = '<input /><button type="button"><span>Open</span></button>';
    input = document.querySelector('input')!;
    button = document.querySelector('button')!;
    (document.activeElement as HTMLElement | null)?.blur();
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('returns the focused element when something has focus', () => {
    input.focus();
    press(button);
    expect(focusReturnTarget()).toBe(input);
  });

  it('returns the pressed control when a click left focus on body', () => {
    press(button.querySelector('span')!);
    expect(focusReturnTarget()).toBe(button);
  });

  it('forgets the press after keyboard use or a real focus move', () => {
    press(button);
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(focusReturnTarget()).toBeNull();

    press(button);
    input.focus();
    input.blur();
    expect(focusReturnTarget()).toBeNull();
  });

  it('ignores a control that keeps focus away on purpose', () => {
    press(button, true);
    expect(focusReturnTarget()).toBeNull();
  });

  it('ignores a pressed control that has left the document', () => {
    press(button);
    button.remove();
    expect(focusReturnTarget()).toBeNull();
  });
});
