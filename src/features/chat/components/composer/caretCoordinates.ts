/**
 * Where a character sits inside a `<textarea>`, in pixels.
 *
 * A textarea exposes offsets, not geometry, so the text is laid out a second
 * time in a hidden div that copies the textarea's box and typography, and the
 * marker span is measured there. This is the price of keeping the composer a
 * textarea instead of a rich editor, and it is a small one: the mirror is built
 * and thrown away only when the popup opens or the query changes.
 */

export interface CaretPoint {
  /** From the textarea's left border, so it can be used as `left` in a box around it. */
  left: number;
  /** From the textarea's top border, already corrected for scrolling. */
  top: number;
}

/** Everything that decides where a line breaks or how wide a glyph is. */
const MIRRORED_PROPERTIES = [
  'box-sizing',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'font-family',
  'font-size',
  'font-weight',
  'font-style',
  'font-variant',
  'letter-spacing',
  'word-spacing',
  'line-height',
  'text-indent',
  'text-transform',
  'tab-size',
] as const;

export function measureCaret(textarea: HTMLTextAreaElement, index: number): CaretPoint {
  const doc = textarea.ownerDocument;
  const view = doc.defaultView;
  const computed = view?.getComputedStyle(textarea);
  if (!computed) return { left: 0, top: 0 };

  const mirror = doc.createElement('div');
  for (const property of MIRRORED_PROPERTIES) {
    mirror.style.setProperty(property, computed.getPropertyValue(property));
  }
  // The mirror carries no border of its own: the border widths are added back
  // at the end so the point is relative to the textarea's border box.
  mirror.style.border = '0';
  mirror.style.position = 'absolute';
  mirror.style.top = '0';
  mirror.style.left = '-9999px';
  mirror.style.visibility = 'hidden';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.overflowWrap = 'break-word';
  mirror.style.width = `${textarea.clientWidth}px`;

  mirror.textContent = textarea.value.slice(0, index);
  const marker = doc.createElement('span');
  // A trailing newline collapses without something after it to hold the line.
  marker.textContent = textarea.value.slice(index) || '.';
  mirror.appendChild(marker);

  doc.body.appendChild(mirror);
  const left = marker.offsetLeft;
  const top = marker.offsetTop;
  doc.body.removeChild(mirror);

  const borderLeft = Number.parseFloat(computed.borderLeftWidth) || 0;
  const borderTop = Number.parseFloat(computed.borderTopWidth) || 0;
  return { left: left + borderLeft, top: top + borderTop - textarea.scrollTop };
}
