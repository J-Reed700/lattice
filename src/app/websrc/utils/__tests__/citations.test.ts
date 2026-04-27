import { describe, it, expect } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import { parseCitations, createCitationMap } from '../citations';

describe('parseCitations', () => {
  it('parses [1] format', () => {
    const result = parseCitations('Paris is the capital[1] of France.');

    expect(result).toEqual([
      { text: 'Paris is the capital', citationNumber: undefined },
      { text: '', citationNumber: 1 },
      { text: ' of France.', citationNumber: undefined },
    ]);
  });

  it('parses multiple citations in [N] format', () => {
    const result = parseCitations('AI[1] and ML[2] are related.');

    expect(result).toHaveLength(5);
    expect(result[0]).toEqual({ text: 'AI', citationNumber: undefined });
    expect(result[1]).toEqual({ text: '', citationNumber: 1 });
    expect(result[2]).toEqual({ text: ' and ML', citationNumber: undefined });
    expect(result[3]).toEqual({ text: '', citationNumber: 2 });
    expect(result[4]).toEqual({ text: ' are related.', citationNumber: undefined });
  });

  it('handles (1) format', () => {
    const result = parseCitations('Test(1) text.');

    expect(result).toHaveLength(3);
    expect(result[1].citationNumber).toBe(1);
  });

  it('handles superscript format', () => {
    const result = parseCitations('Test¹ text.');

    expect(result).toHaveLength(3);
    expect(result[1].citationNumber).toBe(1);
  });

  it('handles multiple superscript digits', () => {
    const result = parseCitations('Test¹²³ text.');

    expect(result).toHaveLength(3);
    expect(result[1].citationNumber).toBe(123);
  });

  it('handles text with no citations', () => {
    const result = parseCitations('Plain text with no citations.');

    expect(result).toEqual([
      { text: 'Plain text with no citations.', citationNumber: undefined },
    ]);
  });

  it('handles empty text', () => {
    const result = parseCitations('');

    expect(result).toEqual([{ text: '', citationNumber: undefined }]);
  });

  it('handles consecutive citations', () => {
    const result = parseCitations('Text[1][2][3] more text.');

    // Consecutive citations don't produce empty text segments between them
    expect(result).toHaveLength(5);
    expect(result[0].text).toBe('Text');
    expect(result[1].citationNumber).toBe(1);
    expect(result[2].citationNumber).toBe(2);
    expect(result[3].citationNumber).toBe(3);
    expect(result[4].text).toBe(' more text.');
  });

  it('handles citation at start of text', () => {
    const result = parseCitations('[1]Text after citation.');

    expect(result).toHaveLength(2);
    expect(result[0].citationNumber).toBe(1);
    expect(result[1].text).toBe('Text after citation.');
  });

  it('handles citation at end of text', () => {
    const result = parseCitations('Text before citation[1]');

    expect(result).toHaveLength(2);
    expect(result[0].text).toBe('Text before citation');
    expect(result[1].citationNumber).toBe(1);
  });

  it('handles mixed citation formats', () => {
    const result = parseCitations('First[1] second(2) third¹ end.');

    expect(result).toHaveLength(7);
    expect(result[1].citationNumber).toBe(1);
    expect(result[3].citationNumber).toBe(2);
    expect(result[5].citationNumber).toBe(1); // ¹ = 1
  });

  it('handles large citation numbers', () => {
    const result = parseCitations('Reference[99] to source.');

    expect(result[1].citationNumber).toBe(99);
  });

  it('parses placeholder citations [#] with source count', () => {
    const result = parseCitations('First[#] second[#] third[#].', 3);

    expect(result).toHaveLength(7);
    expect(result[1].citationNumber).toBe(1);
    expect(result[3].citationNumber).toBe(2);
    expect(result[5].citationNumber).toBe(3);
  });

  it('cycles placeholder citations when placeholders exceed source count', () => {
    const result = parseCitations('A[#] B[#] C[#] D[#].', 2);

    expect(result[1].citationNumber).toBe(1);
    expect(result[3].citationNumber).toBe(2);
    expect(result[5].citationNumber).toBe(1);
    expect(result[7].citationNumber).toBe(2);
  });

  it('normalizes markdown footnote citations [^N]', () => {
    const result = parseCitations('Claim[^1] with more detail[^2].');

    expect(result).toHaveLength(5);
    expect(result[1].citationNumber).toBe(1);
    expect(result[3].citationNumber).toBe(2);
  });
});

describe('createCitationMap', () => {
  const mockSources: SourceWithMetadata[] = [
    {
      documentId: 'doc1',
      chunkId: 'chunk1',
      fileName: 'file1.pdf',
      filePath: '/path/to/file1.pdf',
      mimeType: 'application/pdf',
      category: 'document',
      content: 'Content of first source',
      score: 0.95,
      fileSizeBytes: 1024,
      modifiedAt: '2025-01-01T00:00:00Z',
    },
    {
      documentId: 'doc2',
      chunkId: 'chunk2',
      fileName: 'file2.txt',
      filePath: '/path/to/file2.txt',
      mimeType: 'text/plain',
      category: 'text',
      content: 'Content of second source',
      score: 0.85,
      fileSizeBytes: 512,
      modifiedAt: '2025-01-02T00:00:00Z',
    },
  ];

  it('maps sources to 1-indexed numbers', () => {
    const map = createCitationMap(mockSources);

    expect(map.get(1)?.fileName).toBe('file1.pdf');
    expect(map.get(2)?.fileName).toBe('file2.txt');
  });

  it('returns Map with correct size', () => {
    const map = createCitationMap(mockSources);

    expect(map.size).toBe(2);
  });

  it('preserves all source metadata', () => {
    const map = createCitationMap(mockSources);
    const source1 = map.get(1);

    expect(source1).toEqual(mockSources[0]);
    expect(source1?.documentId).toBe('doc1');
    expect(source1?.score).toBe(0.95);
    expect(source1?.fileSizeBytes).toBe(1024);
  });

  it('handles empty source array', () => {
    const map = createCitationMap([]);

    expect(map.size).toBe(0);
  });

  it('handles single source', () => {
    const map = createCitationMap([mockSources[0]]);

    expect(map.size).toBe(1);
    expect(map.get(1)?.fileName).toBe('file1.pdf');
  });

  it('does not use 0-based indexing', () => {
    const map = createCitationMap(mockSources);

    expect(map.get(0)).toBeUndefined();
    expect(map.get(1)).toBeDefined();
  });

  it('handles many sources', () => {
    const manySources: SourceWithMetadata[] = Array.from({ length: 50 }, (_, i) => ({
      documentId: `doc${i}`,
      chunkId: `chunk${i}`,
      fileName: `file${i}.pdf`,
      filePath: `/path/to/file${i}.pdf`,
      mimeType: 'application/pdf',
      category: 'document',
      content: `Content ${i}`,
      score: 0.9 - i * 0.01,
      fileSizeBytes: 1024,
      modifiedAt: '2025-01-01T00:00:00Z',
    }));

    const map = createCitationMap(manySources);

    expect(map.size).toBe(50);
    expect(map.get(1)?.fileName).toBe('file0.pdf');
    expect(map.get(50)?.fileName).toBe('file49.pdf');
  });
});

describe('parseCitations - Performance & DoS Protection', () => {
  it('should handle very long text efficiently', () => {
    // Keep under 100KB limit (100,000 chars)
    const longText = `${'x'.repeat(49_000)  }[1]${  'y'.repeat(49_000)}`;

    const start = Date.now();
    const result = parseCitations(longText);
    const duration = Date.now() - start;

    expect(duration).toBeLessThan(200); // Should complete in < 200ms
    expect(result).toHaveLength(3); // text + citation + text
  });

  it('should reject text over 100KB', () => {
    const oversized = 'x'.repeat(150_000);

    const result = parseCitations(oversized);

    // Should truncate
    expect(result).toHaveLength(1);
    expect(result[0].citationNumber).toBeUndefined();
    expect(result[0].text).toContain('truncated');
  });

  it('should timeout on excessive citations', () => {
    // Create pathological case: many citations
    const manyCitations = Array.from({ length: 10000 }, (_, i) => `[${i}]`).join(' ');

    const start = Date.now();
    const result = parseCitations(manyCitations);
    const duration = Date.now() - start;

    // Should timeout and return partial results
    expect(duration).toBeLessThan(150); // Timeout at 100ms + overhead
    expect(result.length).toBeGreaterThan(0); // Should have partial results
  });

  it('should reject very long citation numbers', () => {
    // ReDoS attack attempt
    const attack = `[${  '9'.repeat(10000)  }]`;

    const start = Date.now();
    const result = parseCitations(attack);
    const duration = Date.now() - start;

    // Should complete quickly (bounded quantifier prevents backtracking)
    expect(duration).toBeLessThan(50);
    // Should treat as text since >5 digits
    expect(result).toHaveLength(1);
    expect(result[0].citationNumber).toBeUndefined();
  });

  it('should handle max valid citation number (99999)', () => {
    const result = parseCitations('[99999]');
    expect(result).toHaveLength(1);
    expect(result[0].citationNumber).toBe(99999);
  });

  it('should reject citation numbers over 99999', () => {
    const result = parseCitations('[100000]');
    // 6 digits exceeds limit, treated as text
    expect(result).toHaveLength(1);
    expect(result[0].citationNumber).toBeUndefined();
  });
});
