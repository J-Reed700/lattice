import { useEffect, type RefObject } from 'react';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { ConversationTangentDto } from '@/lib/bindings';
import { rangeContainsPoint, setHighlightRanges, supportsCustomHighlights } from '@/utils/customHighlights';
import { flattenRenderedText, renderedRange } from '@/utils/renderedText';

const HIGHLIGHT = 'tangent-source';

/**
 * Where a tangent's passage sits in its rendered source message. The passage
 * was read from a DOM selection, so it may cross inline formatting and line
 * breaks; matching ignores how its whitespace was spelled, and falls back to
 * ignoring whitespace altogether. Only the text is stored, so a passage that
 * occurs twice marks its first occurrence.
 *
 * `null` when the passage is no longer in the message, and when it is the whole
 * message: a tangent on the reply as a whole would tint all of it.
 */
export function findPassageRange(root: HTMLElement, passage: string): Range | null {
  const spacedNeedle = passage.replace(/\s+/g, ' ').trim();
  if (!spacedNeedle) return null;
  const bareNeedle = spacedNeedle.replace(/ /g, '');
  const bare = flattenRenderedText(root, false);
  if (bare.text === bareNeedle) return null;
  const spaced = flattenRenderedText(root, true);
  const spacedAt = spaced.text.indexOf(spacedNeedle);
  if (spacedAt !== -1) return renderedRange(spaced, spacedAt, spacedNeedle.length);
  const bareAt = bare.text.indexOf(bareNeedle);
  return bareAt === -1 ? null : renderedRange(bare, bareAt, bareNeedle.length);
}

/**
 * Keep each tangent's passage marked in the message it was opened from, and
 * open the tangent when its mark is clicked. The marks are CSS Custom
 * Highlights, so the message DOM is never touched; they are rebuilt whenever
 * the message re-renders. Without the API nothing is marked.
 */
export function useTangentSourceHighlights(bodyRef: RefObject<HTMLElement | null>, tangents: readonly ConversationTangentDto[]) {
  useEffect(() => {
    const body = bodyRef.current;
    if (!body || !tangents.length || !supportsCustomHighlights()) return;
    const owner = {};
    let marks: { range: Range; tangent: ConversationTangentDto }[] = [];
    let frame: number | null = null;
    const measure = () => {
      frame = null;
      marks = tangents.flatMap(tangent => {
        const range = findPassageRange(body, tangent.selectedText);
        return range ? [{ range, tangent }] : [];
      });
      setHighlightRanges(HIGHLIGHT, owner, marks.map(mark => mark.range));
    };
    const schedule = () => { if (frame === null) frame = window.requestAnimationFrame(measure); };
    const hit = (event: MouseEvent) => marks.find(mark => rangeContainsPoint(mark.range, event.clientX, event.clientY))?.tangent;
    const open = (event: MouseEvent) => {
      if (event.button !== 0 || event.defaultPrevented) return;
      if (event.target instanceof Element && event.target.closest('a, button')) return;
      // A drag that selected text inside the mark is a selection, not a click.
      if (window.getSelection()?.isCollapsed === false) return;
      const tangent = hit(event);
      if (!tangent) return;
      conversationUiStore.setState({ requestedTangent: { parentId: tangent.parentConversationId, tangentId: tangent.conversationId } });
    };
    const hover = (event: MouseEvent) => {
      const tangent = event.type === 'mouseleave' ? undefined : hit(event);
      if (tangent) {
        body.dataset.tangentHover = '';
        body.title = `Open tangent: ${tangent.title}`;
      } else if (body.dataset.tangentHover !== undefined) {
        delete body.dataset.tangentHover;
        body.removeAttribute('title');
      }
    };

    measure();
    const observer = new MutationObserver(schedule);
    observer.observe(body, { childList: true, characterData: true, subtree: true });
    body.addEventListener('click', open);
    body.addEventListener('mousemove', hover);
    body.addEventListener('mouseleave', hover);
    return () => {
      observer.disconnect();
      if (frame !== null) window.cancelAnimationFrame(frame);
      body.removeEventListener('click', open);
      body.removeEventListener('mousemove', hover);
      body.removeEventListener('mouseleave', hover);
      delete body.dataset.tangentHover;
      body.removeAttribute('title');
      setHighlightRanges(HIGHLIGHT, owner, []);
    };
  }, [bodyRef, tangents]);
}
