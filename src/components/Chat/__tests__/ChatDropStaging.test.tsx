import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '../../ui/tooltip';
import { ChatDropStaging } from '../ChatDropStaging';

const noop = () => {};

describe('ChatDropStaging', () => {
  it('renders nothing when no files are staged', () => {
    const { container } = render(
      <ChatDropStaging
        staged={[]}
        isImporting={false}
        onRemove={noop}
        onImport={noop}
        onClear={noop}
      />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('carries the files, and no longer the scope line', () => {
    // The scope line moved to the linked-documents footer: this row disappears
    // when the import finishes, and the narrowing does not.
    render(
      <TooltipProvider>
        <ChatDropStaging
          staged={[{ path: '/vault/paper.pdf', name: 'paper.pdf' }]}
          isImporting={false}
          onRemove={vi.fn()}
          onImport={vi.fn()}
          onClear={vi.fn()}
        />
      </TooltipProvider>
    );

    expect(screen.getByText('1 file ready')).toBeInTheDocument();
    expect(screen.queryByText(/Search the whole vault/)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/Answers use only this conversation/)
    ).not.toBeInTheDocument();
  });
});
