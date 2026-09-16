import { describe, it, expect } from 'vitest';

import { sanitizeFileName, sanitizeText, sanitizeFilePath } from '../sanitize';

describe('sanitizeFileName', () => {
  it('should pass through normal file names', () => {
    expect(sanitizeFileName('document.pdf')).toBe('document.pdf');
    expect(sanitizeFileName('report_2024.txt')).toBe('report_2024.txt');
    expect(sanitizeFileName('my-file (1).docx')).toBe('my-file (1).docx');
  });

  it('should strip HTML tags', () => {
    expect(sanitizeFileName('doc<b>bold</b>.txt')).toBe('docbold.txt');
    expect(sanitizeFileName('file<span>test</span>.pdf')).toBe('filetest.pdf');
  });

  it('should remove script tags AND their content (secure behavior)', () => {
    // DOMPurify removes script tags AND their content for security
    expect(sanitizeFileName('doc<script>alert(1)</script>.pdf'))
      .toBe('doc.pdf');
  });

  it('should reject javascript URLs', () => {
    expect(sanitizeFileName('javascript:alert(1)')).toBe('[Invalid File Name]');
  });

  it('should reject data URLs', () => {
    expect(sanitizeFileName('data:text/html,<script>alert(1)</script>'))
      .toBe('[Invalid File Name]');
  });

  it('should handle empty string', () => {
    expect(sanitizeFileName('')).toBe('[Unnamed File]');
    expect(sanitizeFileName('   ')).toBe('[Unnamed File]');
  });

  it('should handle HTML entities', () => {
    expect(sanitizeFileName('doc&lt;test&gt;.txt')).toBe('doc&lt;test&gt;.txt');
  });

  it('should preserve Unicode characters', () => {
    expect(sanitizeFileName('文档.pdf')).toBe('文档.pdf');
    expect(sanitizeFileName('dossier_été.txt')).toBe('dossier_été.txt');
  });
});

describe('sanitizeText', () => {
  it('should strip HTML from text', () => {
    expect(sanitizeText('Hello <b>world</b>')).toBe('Hello world');
  });

  it('should remove scripts AND their content (secure behavior)', () => {
    // DOMPurify removes script tags AND their content for security
    expect(sanitizeText('Text<script>alert(1)</script>more'))
      .toBe('Textmore');
  });
});

describe('sanitizeFilePath', () => {
  it('should allow forward slashes in paths', () => {
    expect(sanitizeFilePath('/home/user/document.pdf'))
      .toBe('/home/user/document.pdf');
  });

  it('should strip HTML from paths (removes script tags AND content)', () => {
    // DOMPurify removes script tags AND their content for security
    expect(sanitizeFilePath('/path/<script>alert(1)</script>/file.txt'))
      .toBe('/path//file.txt');
  });

  it('should reject javascript URLs in paths', () => {
    expect(sanitizeFilePath('javascript:alert(1)')).toBe('[Invalid Path]');
  });
});
