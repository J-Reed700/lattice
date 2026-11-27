import { describe, expect, it } from 'vitest';

import { diffDocument, documentOf, draftOf, rebased, type DraftState } from './diffDocument';

import type { FieldSpec } from './types';

const form: FieldSpec[] = [
  { type: 'text', path: '/title', label: 'Title', format: 'line', required: true },
  { type: 'text', path: '/body', label: 'Body', format: 'markdown', required: false },
  {
    type: 'list',
    path: '/choices',
    label: 'Choices',
    itemLabel: 'Choice',
    minItems: 1,
    maxItems: 5,
    item: [
      { type: 'text', path: '/text', label: 'Choice', format: 'plain', required: true },
      { type: 'flag', path: '/correct', label: 'Correct', exclusive: true },
    ],
  },
];

describe('diffDocument', () => {
  it('emits a replace op for changed text and ignores CRLF-only differences', () => {
    const opened = {
      title: 'Quillet',
      body: 'Line one\r\nLine two',
      choices: [{ text: 'Alpha', correct: true }],
    };

    const crlfDraft: DraftState = {
      values: { '/title': 'Quillet', '/body': 'Line one\nLine two' },
      lists: { '/choices': [{ origin: 0, value: { text: 'Alpha', correct: true } }] },
    };
    expect(diffDocument(form, opened, crlfDraft)).toEqual([]);

    const editedDraft: DraftState = {
      values: { '/title': 'Quillet Guide', '/body': 'Line one\nLine two' },
      lists: { '/choices': [{ origin: 0, value: { text: 'Alpha', correct: true } }] },
    };
    expect(diffDocument(form, opened, editedDraft)).toEqual([
      { op: 'replace', path: '/title', value: 'Quillet Guide' },
    ]);
  });

  it('removes highest index first, updates kept leaves at new indices, and adds new members', () => {
    const opened = {
      title: 'Quillet',
      body: 'Body',
      choices: [
        { text: 'Zero', correct: false },
        { text: 'One', correct: true },
        { text: 'Two', correct: false },
        { text: 'Three', correct: false },
      ],
    };

    const draft: DraftState = {
      values: { '/title': 'Quillet', '/body': 'Body' },
      lists: {
        '/choices': [
          { origin: 1, value: { text: 'One updated', correct: true } },
          { origin: 3, value: { text: 'Three', correct: false } },
          { origin: null, value: { text: 'Four', correct: false } },
        ],
      },
    };

    expect(diffDocument(form, opened, draft)).toEqual([
      { op: 'remove', path: '/choices/2' },
      { op: 'remove', path: '/choices/0' },
      { op: 'replace', path: '/choices/0/text', value: 'One updated' },
      { op: 'add', path: '/choices/-', value: { text: 'Four', correct: false } },
    ]);
  });

  it('round-trips draftOf and documentOf and computes positional origins on rebased', () => {
    const doc = {
      title: 'Quillet',
      body: 'Text',
      choices: [
        { text: 'A', correct: true },
        { text: 'B', correct: false },
      ],
    };
    const roundTripped = draftOf(form, doc);
    expect(documentOf(form, roundTripped)).toEqual(doc);

    const draft: DraftState = {
      values: { '/title': 'Mine', '/body': 'Text' },
      lists: {
        '/choices': [
          { origin: 1, value: { text: 'B', correct: false } },
          { origin: null, value: { text: 'C', correct: false } },
          { origin: null, value: { text: 'D', correct: true } },
        ],
      },
    };
    expect(rebased(form, doc, draft).lists['/choices'].map((m) => m.origin)).toEqual([0, 1, null]);
  });
});
