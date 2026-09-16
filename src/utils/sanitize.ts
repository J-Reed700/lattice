import DOMPurify from 'dompurify';

/**
 * Sanitize file name to prevent XSS attacks
 * 
 * @param fileName - Raw file name from file system
 * @returns Sanitized file name safe for rendering
 * 
 * @example
 * sanitizeFileName('report<script>alert(1)</script>.pdf')
 * // Returns: 'reportalert(1).pdf'
 */
export function sanitizeFileName(fileName: string): string {
  // Strip all HTML tags and dangerous content
  const clean = DOMPurify.sanitize(fileName, {
    ALLOWED_TAGS: [], // No HTML tags allowed
    ALLOWED_ATTR: [], // No attributes allowed
    KEEP_CONTENT: true, // Keep text content
  });
  
  // Additional validation: reject javascript: and data: URLs
  const lowerClean = clean.toLowerCase();
  if (lowerClean.includes('javascript:') || lowerClean.includes('data:')) {
    console.warn('[Sanitize] Rejected dangerous file name:', fileName);
    return '[Invalid File Name]';
  }
  
  // Reject if empty after sanitization
  if (!clean.trim()) {
    console.warn('[Sanitize] File name empty after sanitization:', fileName);
    return '[Unnamed File]';
  }
  
  return clean;
}

/**
 * Sanitize arbitrary text content to plain text
 * 
 * @param text - Raw text that may contain HTML
 * @returns Plain text with HTML stripped
 */
export function sanitizeText(text: string): string {
  return DOMPurify.sanitize(text, {
    ALLOWED_TAGS: [], // Plain text only
    ALLOWED_ATTR: [],
    KEEP_CONTENT: true,
  });
}

/**
 * Sanitize file path for display (not for file system operations!)
 * 
 * @param filePath - File path string
 * @returns Sanitized path safe for display
 */
export function sanitizeFilePath(filePath: string): string {
  // Similar to file name but allow / for paths
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
