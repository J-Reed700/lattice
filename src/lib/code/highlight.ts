import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

import './code.css';

/**
 * The one code palette, read from the `--code-*` tokens in code.css, so code
 * reads the same wherever the app shows it. Later rules win where a token
 * carries two tags (a Markdown `#` is both heading and mark).
 */
export const codeHighlighting = syntaxHighlighting(HighlightStyle.define([
  { tag: t.keyword, color: 'var(--code-keyword)', fontWeight: 'var(--code-keyword-weight)' },
  { tag: [t.name, t.variableName], color: 'var(--code-identifier)' },
  { tag: [t.propertyName, t.special(t.variableName)], color: 'var(--code-property)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.standard(t.variableName)], color: 'var(--code-function)' },
  { tag: [t.typeName, t.className, t.namespace], color: 'var(--code-type)' },
  { tag: t.tagName, color: 'var(--code-tag)' },
  { tag: [t.string, t.special(t.string), t.monospace], color: 'var(--code-string)' },
  { tag: [t.number, t.bool, t.null, t.atom, t.unit, t.color], color: 'var(--code-number)' },
  { tag: t.regexp, color: 'var(--code-regexp)' },
  { tag: t.escape, color: 'var(--code-escape)' },
  // Preprocessor lines, macros, attributes, decorators and annotations.
  { tag: [t.meta, t.macroName, t.special(t.name)], color: 'var(--code-meta)' },
  { tag: [t.comment, t.lineComment, t.blockComment], color: 'var(--code-comment)', fontStyle: 'italic' },
  { tag: [t.operator, t.punctuation, t.bracket, t.contentSeparator], color: 'var(--code-punctuation)' },
  { tag: t.heading, color: 'var(--code-heading)', fontWeight: '600' },
  { tag: t.quote, color: 'var(--code-comment)', fontStyle: 'italic' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strong, fontWeight: '600' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.link, color: 'var(--code-link)' },
  { tag: t.url, color: 'var(--code-link)', textDecoration: 'underline', textUnderlineOffset: '2px' },
  { tag: t.processingInstruction, color: 'var(--code-meta)' },
  { tag: t.inserted, color: 'var(--code-inserted)' },
  { tag: t.deleted, color: 'var(--code-deleted)' },
  { tag: t.changed, color: 'var(--code-changed)' },
  { tag: t.invalid, color: 'var(--code-invalid)', textDecoration: 'underline wavy' },
]));
