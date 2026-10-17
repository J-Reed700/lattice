import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { calloutAlertsKey } from '../extensions/calloutAlerts';

import type { DecorationSet } from '@tiptap/pm/view';

function createTestEditor(content: string): Editor {
  return new Editor({
    extensions: [
      ...createExtensions(),
      Markdown.configure({
        html: false,
        transformPastedText: false,
        transformCopiedText: false,
      }),
    ],
    content,
    editable: false,
  });
}

function getCalloutDecorations(editor: Editor) {
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === calloutAlertsKey);
  const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet | undefined;
  return set ? set.find() : [];
}

describe('CalloutAlerts extension', () => {
  it.each([
    ['[!NOTE]', 'callout-note'],
    ['[!TIP]', 'callout-tip'],
    ['[!IMPORTANT]', 'callout-important'],
    ['[!WARNING]', 'callout-warning'],
    ['[!CAUTION]', 'callout-caution'],
    ['[!note]', 'callout-note'],
    ['[!important]', 'callout-important'],
  ])('decorates %s blockquote with %s class', (marker, expectedClass) => {
    const md = `> ${marker}\n> This is the alert message.`;
    const editor = createTestEditor(md);
    const decorations = getCalloutDecorations(editor);

    // Node decoration on blockquote
    const nodeDeco = decorations.find(
      (d: any) => d.type?.attrs?.class?.includes('callout-alert')
    ) as any;
    expect(nodeDeco).toBeDefined();
    expect(nodeDeco.type.attrs.class).toContain('callout-alert');
    expect(nodeDeco.type.attrs.class).toContain(expectedClass);

    // Widget decoration for header
    const widgetDeco = decorations.find((d: any) => d.type?.toDOM !== undefined) as any;
    expect(widgetDeco).toBeDefined();
    const dom = widgetDeco.type.toDOM();
    expect(dom.className).toBe('callout-header');

    // Inline decoration hiding the marker
    const inlineDeco = decorations.find(
      (d: any) => d.type?.attrs?.class === 'callout-marker-hidden'
    );
    expect(inlineDeco).toBeDefined();

    editor.destroy();
  });

  it('leaves standard blockquotes unstyled by callouts', () => {
    const md = `> Just a regular quote without any alert marker.`;
    const editor = createTestEditor(md);
    const decorations = getCalloutDecorations(editor);

    expect(decorations).toHaveLength(0);
    editor.destroy();
  });

  it('handles inline alert on single line', () => {
    const md = `> [!NOTE] This note is all on one line.`;
    const editor = createTestEditor(md);
    const decorations = getCalloutDecorations(editor);

    const nodeDeco = decorations.find(
      (d: any) => d.type?.attrs?.class?.includes('callout-alert')
    ) as any;
    expect(nodeDeco).toBeDefined();
    expect(nodeDeco.type.attrs.class).toContain('callout-note');

    // Marker is hidden
    const inlineDeco = decorations.find(
      (d: any) => d.type?.attrs?.class === 'callout-marker-hidden'
    );
    expect(inlineDeco).toBeDefined();

    editor.destroy();
  });
});
