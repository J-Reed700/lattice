import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

import { MarkdownViewer } from '../MarkdownViewer';

describe('MarkdownViewer', () => {
  it('should render normal markdown', () => {
    const content = '# Hello\n\nThis is **bold** text.';
    render(<MarkdownViewer content={content} />);
    
    expect(screen.getByText('Hello')).toBeInTheDocument();
    expect(screen.getByText('bold')).toBeInTheDocument();
  });

  it('should strip script tags', () => {
    const malicious = '# Title\n\n<script>alert("XSS")</script>';
    render(<MarkdownViewer content={malicious} />);
    
    // Script tag should be removed
    expect(document.querySelector('script')).toBeNull();
  });

  it('should strip iframe tags', () => {
    const malicious = '# Title\n\n<iframe src="https://evil.com"></iframe>';
    render(<MarkdownViewer content={malicious} />);
    
    // Iframe should be removed
    expect(document.querySelector('iframe')).toBeNull();
  });

  it('should strip onerror handlers', () => {
    const malicious = '# Title\n\n<img src=x onerror="alert(1)">';
    const { container } = render(<MarkdownViewer content={malicious} />);
    
    // Image might remain but without onerror
    const img = container.querySelector('img');
    if (img) {
      expect(img.getAttribute('onerror')).toBeNull();
    }
  });

  it('should allow safe HTML like bold and italic', () => {
    const content = '**Bold** and *italic*';
    render(<MarkdownViewer content={content} />);
    
    expect(screen.getByText('Bold')).toBeInTheDocument();
    expect(screen.getByText('italic')).toBeInTheDocument();
  });

  it('should render code blocks safely', () => {
    const content = '```javascript\nconst x = 1;\n```';
    render(<MarkdownViewer content={content} />);
    
    expect(screen.getByText(/const x = 1/)).toBeInTheDocument();
  });

  it('should sanitize links with javascript URLs', () => {
    const malicious = '[Click me](javascript:alert(1))';
    const { container } = render(<MarkdownViewer content={malicious} />);
    
    const link = container.querySelector('a');
    // Link should either be removed or have href sanitized
    if (link) {
      const href = link.getAttribute('href');
      // Should not have javascript: URL (null is also acceptable)
      expect(!href?.includes('javascript:')).toBe(true);
    }
  });

  it('should add target="_blank" and rel="noopener noreferrer" to links', () => {
    const content = '[External Link](https://example.com)';
    const { container } = render(<MarkdownViewer content={content} />);
    
    const link = container.querySelector('a');
    expect(link).toBeTruthy();
    if (link) {
      expect(link.getAttribute('target')).toBe('_blank');
      expect(link.getAttribute('rel')).toBe('noopener noreferrer');
    }
  });

  it('should strip onclick handlers', () => {
    const malicious = '# Title\n\n<div onclick="alert(\'XSS\')">Click me</div>';
    const { container } = render(<MarkdownViewer content={malicious} />);
    
    const div = container.querySelector('div[onclick]');
    expect(div).toBeNull();
  });

  it('should render lists safely', () => {
    const content = '- Item 1\n- Item 2\n- Item 3';
    render(<MarkdownViewer content={content} />);
    
    expect(screen.getByText('Item 1')).toBeInTheDocument();
    expect(screen.getByText('Item 2')).toBeInTheDocument();
    expect(screen.getByText('Item 3')).toBeInTheDocument();
  });

  it('should render tables with GFM', () => {
    const content = '| Col 1 | Col 2 |\n|-------|-------|\n| A | B |';
    render(<MarkdownViewer content={content} />);
    
    expect(screen.getByText('Col 1')).toBeInTheDocument();
    expect(screen.getByText('A')).toBeInTheDocument();
  });

  it('should strip multiple XSS attempts in single document', () => {
    const malicious = `
# Test Document

<script>alert("XSS1")</script>

Normal content

<img src=x onerror="alert('XSS2')">

More content

<iframe src="https://evil.com"></iframe>

[Bad Link](javascript:alert(3))
    `.trim();
    
    const { container } = render(<MarkdownViewer content={malicious} />);
    
    expect(document.querySelector('script')).toBeNull();
    expect(document.querySelector('iframe')).toBeNull();
    
    const img = container.querySelector('img');
    if (img) {
      expect(img.getAttribute('onerror')).toBeNull();
    }
    
    const links = container.querySelectorAll('a');
    links.forEach(link => {
      const href = link.getAttribute('href');
      // Should not have javascript: URL (null is also acceptable)
      expect(!href?.includes('javascript:')).toBe(true);
    });
  });
});
