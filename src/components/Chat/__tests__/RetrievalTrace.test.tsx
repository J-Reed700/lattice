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

  it('shows recovered tool results without inventing a searched document count', () => {
    const { container } = render(<RetrievalTrace trace={{ searchedDocuments: 0, passages: 7, files: 7, scope: 'vault' }} />);
    expect(container.textContent).toBe('Retrieved 7 passages from 7 files');
    expect(screen.queryByText(/Searched 0/)).not.toBeInTheDocument();
  });

  it('shows a known document count when historical passage counts are unavailable', () => {
    const { container } = render(<RetrievalTrace trace={{ searchedDocuments: 0, passages: 0, files: 7, scope: 'vault' }} />);
    expect(container.textContent).toBe('Retrieved 7 files');
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

  it('says nothing extra when retrieval went the ordinary way', () => {
    // A sufficient verdict is the expected case. Announcing it on every answer
    // would be noise, and an absent verdict is not a failed one.
    const { container } = render(
      <RetrievalTrace
        trace={{
          searchedDocuments: 12,
          passages: 4,
          files: 2,
          scope: 'vault',
          kbSufficient: true,
          kbCorrectiveRetries: 0,
          kbPlannerSkipped: false,
        }}
      />
    );
    expect(container.textContent).toBe('Searched 12 documents · 4 passages from 2 files');
  });

  it('notes a corrective pass', () => {
    render(
      <RetrievalTrace
        trace={{
          searchedDocuments: 12,
          passages: 4,
          files: 2,
          scope: 'vault',
          kbSufficient: false,
          kbCorrectiveRetries: 1,
          kbPlannerSkipped: false,
          sufficiencyReasons: ['low_term_coverage'],
        }}
      />
    );
    expect(screen.getByText(/searched again/)).toBeInTheDocument();
  });

  it('notes a skipped planner, and combines it with a corrective pass', () => {
    const { rerender } = render(
      <RetrievalTrace
        trace={{
          searchedDocuments: 12,
          passages: 4,
          files: 2,
          scope: 'vault',
          kbPlannerSkipped: true,
        }}
      />
    );
    expect(screen.getByText(/Reused the previous topic instead/)).toBeInTheDocument();

    rerender(
      <RetrievalTrace
        trace={{
          searchedDocuments: 12,
          passages: 4,
          files: 2,
          scope: 'vault',
          kbPlannerSkipped: true,
          kbCorrectiveRetries: 1,
        }}
      />
    );
    expect(screen.getByText(/Reused the previous topic, then searched again/)).toBeInTheDocument();
  });

  it('says so only when the conversation is scoped', () => {
    const { rerender } = render(
      <RetrievalTrace trace={{ searchedDocuments: 4, passages: 2, files: 1, scope: 'linked' }} />
    );
    expect(screen.getByText(/this space/s)).toBeInTheDocument();

    rerender(
      <RetrievalTrace trace={{ searchedDocuments: 4, passages: 2, files: 1, scope: 'vault' }} />
    );
    expect(screen.queryByText(/this space/s)).not.toBeInTheDocument();
  });
});
