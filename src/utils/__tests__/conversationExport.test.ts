import { describe, expect, it } from 'vitest';

import type { ClaimVerdict, MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';

import {
  answerToMarkdown,
  askWhyPrefill,
  claimToMarkdown,
  claimWithCitation,
  conversationToMarkdown,
  isWebSource,
  vaultDocumentIds,
  verificationLine,
  type ExportableMessage,
} from '../conversationExport';


const AT = new Date('2026-09-19T14:30:00.000Z');

function source(overrides: Partial<SourceWithMetadata> = {}): SourceWithMetadata {
  return {
    documentId: 'doc-halvorsen',
    chunkId: 'doc-halvorsen#c1',
    fileName: 'Halvorsen 2024.pdf',
    filePath: '/vault/Papers/Halvorsen 2024.pdf',
    mimeType: 'application/pdf',
    category: 'Research Paper',
    content: 'The pooled estimate was 1.2 °C.',
    excerpt: 'The pooled estimate was 1.2 °C.',
    score: 0.91,
    fileSizeBytes: 1_480_000,
    modifiedAt: '2026-08-01T00:00:00.000Z',
    pageNumber: 14,
    citationId: 1,
    ...overrides,
  };
}

const transect = source({
  documentId: 'doc-transect',
  chunkId: 'doc-transect#c3',
  fileName: 'Riverside Transect - July.md',
  filePath: '/vault/Field Notes/Riverside Transect - July.md',
  mimeType: 'text/markdown',
  category: 'Notes',
  pageNumber: undefined,
  section: 'Afternoon pass',
  citationId: 2,
});

const webPage = source({
  documentId: 'web:https://example.org/heat-equity',
  chunkId: 'web#c1',
  fileName: 'What the heat maps leave out',
  filePath: 'https://example.org/heat-equity',
  mimeType: 'text/html',
  category: 'Web Article',
  pageNumber: undefined,
  citationId: 3,
});

const verification: MessageVerificationSummary = {
  enabled: true,
  claimsEvaluated: 5,
  supportedClaims: 4,
  unsupportedClaims: ['Thermal imagery overstates what residents feel by a factor of three.'],
  claimVerdicts: [],
};

function message(overrides: Partial<ExportableMessage> = {}): ExportableMessage {
  return {
    id: 'm1',
    role: 'assistant',
    content: 'About 1.2 °C per 10 points of canopy [1].',
    createdAt: '2026-09-19T14:00:00.000Z',
    ...overrides,
  };
}

describe('conversationToMarkdown', () => {
  it('heads the export with the title, the space and when the chat was last touched', () => {
    const markdown = conversationToMarkdown(
      { title: 'How much cooling does canopy buy?', updatedAt: '2026-09-19T14:00:00.000Z' },
      [message({ role: 'user', content: 'What do my sources say?' })],
      { spaceName: 'Heat Island Thesis', now: AT },
    );

    expect(markdown).toContain('# How much cooling does canopy buy?');
    expect(markdown).toContain('Heat Island Thesis · 1 message · updated');
  });

  it('names an untitled conversation rather than printing a bare hash', () => {
    const markdown = conversationToMarkdown({ title: '   ' }, [], { now: AT });
    expect(markdown).toContain('# Untitled conversation');
  });

  it('labels each turn by who spoke', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ id: 'm0', role: 'user', content: 'What do my sources say?' }),
      message(),
    ], { now: AT });

    expect(markdown).toContain('## You\n\nWhat do my sources say?');
    expect(markdown).toContain('## Assistant');
  });

  it('follows an answer with its sources, numbered as the answer cites them', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ sources: [source(), transect] }),
    ], { now: AT });

    expect(markdown).toContain('**Sources**');
    expect(markdown).toContain('1. Halvorsen 2024.pdf — PDF p. 14');
    expect(markdown).toContain('2. Riverside Transect - July.md — § Afternoon pass');
  });

  it('prints a web source as its address, since it has no page or section', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ sources: [webPage] }),
    ], { now: AT });

    expect(markdown).toContain('3. What the heat maps leave out — https://example.org/heat-equity');
  });

  it('says what the sentence check found, and names what it could not ground', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ metadata: JSON.stringify({ verification }) }),
    ], { now: AT });

    expect(markdown).toContain(
      '_4 of 5 checked sentences backed; not found: “Thermal imagery overstates what residents feel by a factor of three.”._',
    );
  });

  it('reads sources and verification out of the message metadata when no store map is given', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ metadata: JSON.stringify({ sources: [source()], verification }) }),
    ], { now: AT });

    expect(markdown).toContain('1. Halvorsen 2024.pdf — PDF p. 14');
    expect(markdown).toContain('4 of 5 checked sentences backed');
  });

  it('prefers the store maps, which are what the reader was looking at', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ metadata: JSON.stringify({ sources: [source()] }) }),
    ], {
      sources: new Map([['m1', [transect]]]),
      now: AT,
    });

    expect(markdown).toContain('2. Riverside Transect - July.md');
    expect(markdown).not.toContain('Halvorsen 2024.pdf');
  });

  it('survives metadata that is not JSON', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ metadata: '{ this is not json' }),
    ], { now: AT });

    expect(markdown).toContain('About 1.2 °C per 10 points of canopy [1].');
    expect(markdown).not.toContain('**Sources**');
  });

  it('says the model and the time when a turn record has landed', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({
        metadata: JSON.stringify({
          turn: { model: { id: 'qwen3-8b', name: 'Qwen3 8B' }, timing: { totalMs: 12_400 } },
        }),
      }),
    ], { now: AT });

    expect(markdown).toContain('_Answered by Qwen3 8B · 12.4 s._');
  });

  it('says nothing about the model on a turn persisted before turn records existed', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [message()], { now: AT });
    expect(markdown).not.toContain('Answered by');
  });

  it('ignores a turn record that carries neither a model nor a time', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ metadata: JSON.stringify({ turn: { steps: [], model: null } }) }),
    ], { now: AT });

    expect(markdown).not.toContain('Answered by');
  });

  it('leaves out a message with nothing in it', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [
      message({ id: 'm0', role: 'user', content: '   ' }),
      message(),
    ], { now: AT });

    expect(markdown).not.toContain('## You');
    expect(markdown).toContain('1 message');
  });

  it('ends with exactly one newline, so a paste does not trail blank lines', () => {
    const markdown = conversationToMarkdown({ title: 'Canopy' }, [message()], { now: AT });
    expect(markdown.endsWith('\n')).toBe(true);
    expect(markdown.endsWith('\n\n')).toBe(false);
  });
});

describe('verificationLine', () => {
  it('stays silent when verification was off', () => {
    expect(verificationLine({ ...verification, enabled: false })).toBeNull();
  });

  it('stays silent when there was nothing to check, rather than reporting 0 of 0', () => {
    expect(verificationLine({ enabled: true, claimsEvaluated: 0 })).toBeNull();
  });

  it('says so plainly when every checked sentence was backed', () => {
    expect(verificationLine({ enabled: true, claimsEvaluated: 3, unsupportedClaims: [] })).toBe(
      '3 of 3 checked sentences backed.',
    );
  });

  it('counts the rest instead of listing every ungrounded sentence', () => {
    const line = verificationLine({
      enabled: true,
      claimsEvaluated: 9,
      unsupportedClaims: ['one', 'two', 'three', 'four', 'five'],
    });

    expect(line).toBe('4 of 9 checked sentences backed; not found: “one”; “two”; “three”; and 2 more.');
  });
});

describe('vaultDocumentIds', () => {
  it('lists each vault document once, in citation order', () => {
    expect(vaultDocumentIds([source(), source({ chunkId: 'other' }), transect])).toEqual([
      'doc-halvorsen',
      'doc-transect',
    ]);
  });

  it('leaves web pages out: Compare reads files', () => {
    expect(isWebSource(webPage)).toBe(true);
    expect(vaultDocumentIds([source(), webPage])).toEqual(['doc-halvorsen']);
  });
});

describe('answerToMarkdown', () => {
  it('says where the answer came from and keeps its sources', () => {
    const markdown = answerToMarkdown({
      content: 'About 1.2 °C per 10 points [1].',
      sources: [source()],
      verification,
      conversationTitle: 'How much cooling does canopy buy?',
      capturedAt: AT,
    });

    expect(markdown).toContain('## From chat · How much cooling does canopy buy?');
    expect(markdown).toContain('About 1.2 °C per 10 points [1].');
    expect(markdown).toContain('1. Halvorsen 2024.pdf — PDF p. 14');
    expect(markdown).toContain('4 of 5 checked sentences backed');
  });
});

const backed: ClaimVerdict = {
  sentence: 'The pooled estimate was 1.2 °C per 10 points of canopy.',
  citationIds: [1],
  verdict: 'supported',
  evidenceQuote: 'the pooled estimate was a 1.2 °C reduction',
  method: 'judge',
};

const citationMap = new Map([
  [1, source()],
  [2, transect],
]);

describe('claimWithCitation', () => {
  it('carries the sentence and where it came from', () => {
    expect(claimWithCitation(backed, citationMap)).toBe(
      'The pooled estimate was 1.2 °C per 10 points of canopy.\n\n— Halvorsen 2024.pdf, PDF p. 14',
    );
  });

  it('copies an uncited sentence alone rather than implying a source', () => {
    expect(claimWithCitation({ ...backed, citationIds: [] }, citationMap)).toBe(
      'The pooled estimate was 1.2 °C per 10 points of canopy.',
    );
  });
});

describe('claimToMarkdown', () => {
  it('writes the quote, its source, the evidence and the verdict', () => {
    const markdown = claimToMarkdown(backed, citationMap, {
      conversationTitle: 'Canopy',
      capturedAt: AT,
    });

    expect(markdown).toContain('> The pooled estimate was 1.2 °C per 10 points of canopy.');
    expect(markdown).toContain('— Halvorsen 2024.pdf, PDF p. 14');
    expect(markdown).toContain('Evidence: “the pooled estimate was a 1.2 °C reduction”');
    expect(markdown).toContain('_Backed by the source — the model read the passage against it._');
  });

  it('says a lexical match was never read for meaning', () => {
    const markdown = claimToMarkdown({ ...backed, method: 'lexical' }, citationMap, { capturedAt: AT });
    expect(markdown).toContain('matched on shared wording, not read for meaning');
  });
});

describe('askWhyPrefill', () => {
  it('asks an ungrounded sentence for a source, not for its reasoning', () => {
    const prefill = askWhyPrefill({ ...backed, verdict: 'unsupported', citationIds: [] });
    expect(prefill).toContain('Where in my documents does this come from?');
  });

  it('asks a backed sentence which words it rests on', () => {
    expect(askWhyPrefill(backed)).toContain('Quote the words it rests on.');
  });

  it('asks a contradicted sentence what the passage actually says', () => {
    expect(askWhyPrefill({ ...backed, verdict: 'contradicted' })).toContain('what does it actually say?');
  });
});
