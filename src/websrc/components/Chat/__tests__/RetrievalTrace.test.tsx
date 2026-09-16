import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { RetrievalTrace } from '../RetrievalTrace';

describe('RetrievalTrace', () => {
  it('renders nothing when there is no trace', () => {
    // Absent is not zero: a turn without retrieval did not search nothing,
    // it did not search.
    const { container } = render(<RetrievalTrace trace={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when the knowledge base was skipped', () => {
    // The backend still sends a trace carrying `unavailableReason` so the
    // composer can say why. "Searched 0 documents" would read as a search that
    // came up empty, which is a different claim and an untrue one.
    const { container } = render(
      <RetrievalTrace
        trace={{
          searchedDocuments: 0,
          passages: 0,
          files: 0,
          scope: 'vault',
          unavailableReason: 'the embedding model is not ready',
        }}
      />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('renders the document, passage and file counts', () => {
    const { container } = render(
      <RetrievalTrace
        trace={{ searchedDocuments: 1247, passages: 6, files: 3, scope: 'vault' }}
      />
    );
    expect(container.textContent).toBe(
      'Searched 1,247 documents · 6 passages from 3 files'
    );
  });

  it('uses singular forms for one of each', () => {
    render(
      <RetrievalTrace trace={{ searchedDocuments: 1, passages: 1, files: 1, scope: 'vault' }} />
    );
    const line = screen.getByText(/Searched/s);
    expect(line.textContent).toContain('1 document ');
    expect(line.textContent).toContain('1 passage from');
    expect(line.textContent).toContain('1 file');
  });

  it('omits the passage clause when nothing was retrieved', () => {
    render(
      <RetrievalTrace trace={{ searchedDocuments: 12, passages: 0, files: 0, scope: 'vault' }} />
    );
    expect(screen.getByText(/Searched/s).textContent).not.toContain('passage');
  });

  it('says so only when the conversation is scoped', () => {
    const { rerender } = render(
      <RetrievalTrace trace={{ searchedDocuments: 4, passages: 2, files: 1, scope: 'linked' }} />
    );
    expect(screen.getByText(/this conversation only/s)).toBeInTheDocument();

    rerender(
      <RetrievalTrace trace={{ searchedDocuments: 4, passages: 2, files: 1, scope: 'vault' }} />
    );
    expect(screen.queryByText(/this conversation only/s)).not.toBeInTheDocument();
  });
});
