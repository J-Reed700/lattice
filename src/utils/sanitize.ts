import DOMPurify from 'dompurify';

export function sanitizeFileName(fileName: string): string {
  const clean = DOMPurify.sanitize(fileName, {
    ALLOWED_TAGS: [],
    ALLOWED_ATTR: [],
    KEEP_CONTENT: true,
  });

  const lowerClean = clean.toLowerCase();
  if (lowerClean.includes('javascript:') || lowerClean.includes('data:')) {
    console.warn('[Sanitize] Rejected dangerous file name:', fileName);
    return '[Invalid File Name]';
  }

  if (!clean.trim()) {
    console.warn('[Sanitize] File name empty after sanitization:', fileName);
    return '[Unnamed File]';
  }

  return clean;
}

export function sanitizeText(text: string): string {
  return DOMPurify.sanitize(text, {
    ALLOWED_TAGS: [],
    ALLOWED_ATTR: [],
    KEEP_CONTENT: true,
  });
}

/** Sanitize file path for display (not for file system operations). */
export function sanitizeFilePath(filePath: string): string {
  const clean = DOMPurify.sanitize(filePath, {
    ALLOWED_TAGS: [],
    ALLOWED_ATTR: [],
    KEEP_CONTENT: true,
  });

  if (clean.toLowerCase().includes('javascript:') || clean.toLowerCase().includes('data:')) {
    return '[Invalid Path]';
  }

  return clean || '[Unknown Path]';
}
