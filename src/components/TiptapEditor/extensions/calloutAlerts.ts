import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

export type CalloutAlertType = 'NOTE' | 'TIP' | 'IMPORTANT' | 'WARNING' | 'CAUTION';

export const calloutAlertsKey = new PluginKey('calloutAlerts');

const ALERT_REGEX = /^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\](?:\r?\n|[ \t]+)?/i;

const ALERT_TITLES: Record<CalloutAlertType, string> = {
  NOTE: 'Note',
  TIP: 'Tip',
  IMPORTANT: 'Important',
  WARNING: 'Warning',
  CAUTION: 'Caution',
};

// Clean, accessible SVG icons matching GitHub alert semantics
const ALERT_ICONS: Record<CalloutAlertType, string> = {
  NOTE: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <circle cx="8" cy="8" r="6.5" />
    <line x1="8" y1="7" x2="8" y2="11.5" />
    <circle cx="8" cy="4.5" r="0.75" fill="currentColor" stroke="none" />
  </svg>`,
  TIP: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M6 12.5h4M6.5 14h3" />
    <path d="M8 2a4.5 4.5 0 0 0-3 7.85v1.4a.75.75 0 0 0 .75.75h4.5a.75.75 0 0 0 .75-.75v-1.4A4.5 4.5 0 0 0 8 2z" />
  </svg>`,
  IMPORTANT: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M8 1.5c-2 2.5-3.5 4.5-3.5 7a3.5 3.5 0 0 0 7 0c0-1.5-.5-2.8-1.5-4-.5 2-1.5 2.5-2 2.5.5-1.5.5-3.5 0-5.5z" />
  </svg>`,
  WARNING: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M7.13 2.67a1 1 0 0 1 1.74 0l5.5 9.5A1 1 0 0 1 13.5 13.5H2.5a1 1 0 0 1-.87-1.33l5.5-9.5z" />
    <line x1="8" y1="6" x2="8" y2="9.5" />
    <circle cx="8" cy="11.5" r="0.6" fill="currentColor" stroke="none" />
  </svg>`,
  CAUTION: `<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <polygon points="5.2 2 10.8 2 14 5.2 14 10.8 10.8 14 5.2 14 2 10.8 2 5.2" />
    <line x1="8" y1="5.5" x2="8" y2="9" />
    <circle cx="8" cy="11" r="0.6" fill="currentColor" stroke="none" />
  </svg>`,
};

function createCalloutHeader(type: CalloutAlertType): HTMLElement {
  const container = document.createElement('div');
  container.className = 'callout-header';
  container.setAttribute('contenteditable', 'false');
  container.setAttribute('aria-label', ALERT_TITLES[type]);

  const iconSpan = document.createElement('span');
  iconSpan.className = 'callout-icon';
  iconSpan.innerHTML = ALERT_ICONS[type];

  const titleSpan = document.createElement('span');
  titleSpan.className = 'callout-title';
  titleSpan.textContent = ALERT_TITLES[type];

  container.appendChild(iconSpan);
  container.appendChild(titleSpan);
  return container;
}

/**
 * Parses GitHub-style alert callouts from blockquotes:
 *
 * > [!NOTE]
 * > Informational message here
 *
 * Supported types: NOTE, TIP, IMPORTANT, WARNING, CAUTION
 * Non-destructive: uses ProseMirror decorations so original markdown text
 * and round-tripping remain completely intact.
 */
export const CalloutAlerts = Extension.create({
  name: 'calloutAlerts',

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: calloutAlertsKey,
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];

            state.doc.descendants((node, pos) => {
              if (node.type.name !== 'blockquote') return;

              const firstChild = node.firstChild;
              if (!firstChild) return;

              const text = firstChild.textContent;
              const match = text.match(ALERT_REGEX);
              if (!match) return;

              const alertType = match[1].toUpperCase() as CalloutAlertType;

              // 1. Decorate the blockquote container with callout classes
              decorations.push(
                Decoration.node(pos, pos + node.nodeSize, {
                  class: `callout-alert callout-${alertType.toLowerCase()}`,
                  'data-callout-type': alertType.toLowerCase(),
                })
              );

              // 2. Insert callout header (icon + title) as first child widget inside blockquote
              decorations.push(
                Decoration.widget(pos + 1, () => createCalloutHeader(alertType), {
                  side: -1,
                  key: `callout-header-${pos}`,
                })
              );

              // 3. Hide the [!TYPE] marker in the first child text
              if (firstChild.firstChild && firstChild.firstChild.isText && firstChild.firstChild.text) {
                const textNode = firstChild.firstChild;
                const textMatch = textNode.text?.match(ALERT_REGEX);
                if (textMatch) {
                  const matchIndex = textMatch.index ?? 0;
                  const markerFrom = pos + 2 + matchIndex;
                  const markerTo = markerFrom + textMatch[0].length;

                  decorations.push(
                    Decoration.inline(markerFrom, markerTo, {
                      class: 'callout-marker-hidden',
                    })
                  );
                }
              }

              // 4. If the first child paragraph contained ONLY the marker, collapse it
              if (text.trim() === match[0].trim()) {
                decorations.push(
                  Decoration.node(pos + 1, pos + 1 + firstChild.nodeSize, {
                    class: 'callout-lead-empty',
                  })
                );
              }
            });

            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
