/**
 * Named CSS Custom Highlights shared by several owners.
 *
 * `CSS.highlights` is one registry per document, keyed by the name a
 * `::highlight(name)` rule styles. Several components mark text under one name
 * at once (each message its own tangent passages; the chat and the tangent
 * beside it their own find matches), so each owner sets only its own ranges and
 * the named highlight is rebuilt from all of them.
 *
 * A highlight paints over the DOM rather than inserting elements into it, so it
 * never disturbs the React- and ProseMirror-owned nodes it marks. Where the API
 * is missing every call is a no-op and nothing is marked.
 */

const owners = new Map<string, Map<object, Range[]>>();

export function supportsCustomHighlights(): boolean {
  return typeof CSS !== 'undefined' && 'highlights' in CSS && typeof Highlight === 'function';
}

/** Replace `owner`'s ranges under `name`; an empty list removes them. */
export function setHighlightRanges(name: string, owner: object, ranges: Range[]): void {
  if (!supportsCustomHighlights()) return;
  const byOwner = owners.get(name) ?? new Map<object, Range[]>();
  if (ranges.length) byOwner.set(owner, ranges);
  else byOwner.delete(owner);
  if (byOwner.size === 0) {
    owners.delete(name);
    CSS.highlights.delete(name);
    return;
  }
  owners.set(name, byOwner);
  CSS.highlights.set(name, new Highlight(...Array.from(byOwner.values()).flat()));
}

/** Whether the point lies on any line box of the range. */
export function rangeContainsPoint(range: Range, x: number, y: number): boolean {
  return Array.from(range.getClientRects()).some(rect =>
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom);
}
