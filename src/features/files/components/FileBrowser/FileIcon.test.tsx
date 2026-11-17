import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { FileIcon } from './FileIcon';

const file = (overrides: Partial<DocumentMetadata> = {}) =>
  ({
    id: 'd1', fileName: 'article.md', filePath: 'C:/data/articles/x/article.md', fileType: 'md', category: 'Notes',
    web: false, typeBucket: 'Markdown', ...overrides,
  }) as DocumentMetadata;

describe('FileIcon', () => {
  it('shows the globe for a document the web archive owns, wherever it is stored', () => {
    const { container } = render(<FileIcon file={file({ web: true, typeBucket: 'Web' })} />);
    expect(container.querySelector('svg.lucide-globe')).not.toBeNull();
    expect(container).not.toHaveTextContent('MD');
  });

  it('shows the format label for a local document', () => {
    const { container } = render(<FileIcon file={file()} />);
    expect(container.querySelector('svg.lucide-globe')).toBeNull();
    expect(container).toHaveTextContent('MD');
  });
});
