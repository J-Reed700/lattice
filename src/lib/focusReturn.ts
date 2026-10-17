/**
 * Where focus should go back to when a dialog or panel that is opening now
 * closes again.
 *
 * Usually that is `document.activeElement`. macOS WebKit (Safari, and the
 * WKWebView the desktop app runs in) does not focus a button or link when it
 * is clicked, so after a click that opens something `document.activeElement`
 * is `<body>`. In that case the control the pointer last pressed is the opener.
 * Chromium focuses the clicked control, so this changes nothing there.
 */
const PRESSABLE = 'button, a[href], summary, input, select';

let lastPressed: HTMLElement | null = null;

if (typeof window !== 'undefined') {
  // Bubble phase, so a control whose mousedown is default-prevented (it keeps
  // focus where it was on purpose) is not recorded.
  window.addEventListener('mousedown', (event) => {
    lastPressed = !event.defaultPrevented && event.target instanceof Element
      ? event.target.closest<HTMLElement>(PRESSABLE)
      : null;
  });
  // Any later keyboard use or real focus move supersedes the press.
  window.addEventListener('keydown', () => { lastPressed = null; }, true);
  window.addEventListener('focusin', () => { lastPressed = null; }, true);
}

export function focusReturnTarget(): HTMLElement | null {
  const active = document.activeElement;
  if (active instanceof HTMLElement && active !== document.body) return active;
  return lastPressed?.isConnected ? lastPressed : null;
}
